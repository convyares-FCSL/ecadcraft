//! Damage below ECAD's records: a damaged page that makes the storage engine (redb) panic never ends the
//! process. Opening such a file is refused in words; a panic in a read after open (an earlier state, undo past
//! the snapshot) is an error and the session goes on.
//!
//! - **Corpus** (`tests/damaged/redb-panics.txt`): one damaged copy of the golden project per redb panic site
//!   seen, kept as byte patches to the golden file.
//! - **A read after open**: a storage backend that panics on demand, armed after the project opened.
//! - **Seeded probe**, in the slow set: random damage (flipped bits, random blocks, zeroed 4 KiB, truncation) to
//!   the golden file, each copy opened, read back and verified. `ECAD_DAMAGE_ROUNDS` sets the rounds (default
//!   300); a failure prints its round's seed.

use std::io;
use std::ops::Bound;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common::*;
use crate::container::files::TempDir;
use ecad_file::disk::DiskProject;
use ecad_file::{Project, Store, StoreError};
use ecad_model::*;
use ecad_store::power::Rng;

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/project.ecad");
const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/damaged/redb-panics.txt");

/// One corpus case: its name, the redb line that panics, and the damaged bytes.
fn corpus() -> Vec<(String, String, Vec<u8>)> {
    let golden = std::fs::read(GOLDEN).unwrap();
    let text = std::fs::read_to_string(CORPUS).unwrap();
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next().unwrap().to_string();
            let site = words.next().unwrap().to_string();
            let mut bytes = golden.clone();
            for patch in words {
                let (at, value) = patch.split_once('=').unwrap();
                match at.split_once('+') {
                    Some((at, len)) => {
                        let (at, len): (usize, usize) = (at.parse().unwrap(), len.parse().unwrap());
                        bytes[at..at + len].fill(0);
                    }
                    None => {
                        let at: usize = at.parse().unwrap();
                        let value: Vec<u8> = (0..value.len()).step_by(2).map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap()).collect();
                        bytes[at..at + value.len()].copy_from_slice(&value);
                    }
                }
            }
            (name, site, bytes)
        })
        .collect()
}

#[test]
fn every_file_of_the_damaged_page_corpus_is_refused_in_words_never_a_crash() {
    let cases = corpus();
    assert!(cases.len() >= 10, "one case per redb panic site");
    let dir = TempDir::new("damaged-pages");
    for (name, site, bytes) in cases {
        let p = dir.path(&format!("{name}.ecad"));
        for read_only in [false, true] {
            std::fs::write(&p, &bytes).unwrap();
            let opened = if read_only { DiskProject::open_read_only(&p, SEED, WRITER) } else { DiskProject::open(&p, SEED, WRITER) };
            let said = match opened {
                Ok(_) => panic!("{name}: a page that stops the storage engine opens"),
                Err(e) => e.to_string(),
            };
            // redb's repair on a writable open can stop elsewhere; read-only it stops where the corpus says.
            assert!(said.contains("the file is damaged and cannot be read"), "{name} (read-only {read_only}): {said}");
            assert!(!read_only || said.contains(&site), "{name}: {said}");
        }
        // Opened read-only, the file is kept exactly as it is. (A writable open lets redb update its own header,
        // as it does for every file.)
        assert!(std::fs::read(&p).unwrap() == bytes, "{name}: changed by a read-only open");
    }
}

/// A storage backend over memory that panics on every read once armed, as redb does on a damaged page.
#[derive(Debug, Clone, Default)]
struct Panicking {
    bytes: ecad_store::MemoryBackend,
    armed: Arc<AtomicBool>,
}

impl redb::StorageBackend for Panicking {
    fn len(&self) -> io::Result<u64> {
        self.bytes.len()
    }
    fn read(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        if self.armed.load(Ordering::SeqCst) {
            panic!("a damaged page at {offset}");
        }
        self.bytes.read(offset, out)
    }
    fn set_len(&self, len: u64) -> io::Result<()> {
        self.bytes.set_len(len)
    }
    fn sync_data(&self) -> io::Result<()> {
        self.bytes.sync_data()
    }
    fn write(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.bytes.write(offset, data)
    }
    fn try_lock_range(&self, a: Bound<u64>, b: Bound<u64>) -> std::result::Result<bool, redb::BackendError> {
        self.bytes.try_lock_range(a, b)
    }
    fn try_lock_shared_range(&self, a: Bound<u64>, b: Bound<u64>) -> std::result::Result<bool, redb::BackendError> {
        self.bytes.try_lock_shared_range(a, b)
    }
    fn lock_range(&self, a: Bound<u64>, b: Bound<u64>) -> std::result::Result<(), redb::BackendError> {
        self.bytes.lock_range(a, b)
    }
    fn lock_shared_range(&self, a: Bound<u64>, b: Bound<u64>) -> std::result::Result<(), redb::BackendError> {
        self.bytes.lock_shared_range(a, b)
    }
    fn unlock_range(&self, a: Bound<u64>, b: Bound<u64>) -> std::result::Result<(), redb::BackendError> {
        self.bytes.unlock_range(a, b)
    }
    fn query_lock_range(&self, a: Bound<u64>, b: Bound<u64>) -> std::result::Result<bool, redb::BackendError> {
        self.bytes.query_lock_range(a, b)
    }
}

#[test]
fn a_panic_in_a_read_after_open_is_an_error_and_the_session_goes_on() {
    // Enough edits that the early transactions are on pages opening never reads.
    let mut f = fixture();
    for i in 0..400 {
        f.commit("Property", vec![Op::SetProperty { object: ObjectRef::Project, key: format!("k{i}"), before: None, after: Some("v".into()) }]);
    }
    let backend = Panicking::default();
    let (p, _) = Project::create(Store::with_backend(backend.clone()).unwrap(), PROJECT, &f.ledger, Some(1), WRITER, 2_000).unwrap();
    drop(p);
    // Read-only, as a file another session holds: redb reads pages only when asked.
    let mut o = Project::open(Store::with_backend_read_only(backend.clone()).unwrap(), SEED, WRITER).unwrap();
    assert_eq!((o.report.snapshot_seq, o.report.damage.clone()), (f.ledger.head(), None), "opened from the head snapshot");
    let shown = o.ledger.document().clone();

    // An earlier state reads transactions before the snapshot: the engine panics on the page.
    backend.armed.store(true, Ordering::SeqCst);
    let e = o.ledger.state_at(1).expect_err("the read fails");
    assert!(e.to_string().contains("the file is damaged and cannot be read") && e.to_string().contains("a damaged page"), "{e}");
    backend.armed.store(false, Ordering::SeqCst);
    let halted = o.project.store().halted();
    assert!(matches!(halted, Some(StoreError::Halted(_))), "{halted:?}");

    // The session goes on: what is shown is unchanged, edits still apply in memory, and the halted file is
    // never read again (the backend is disarmed, yet the read still fails), nor written.
    assert_eq!(o.ledger.document(), &shown);
    o.ledger
        .commit(
            Author::User,
            "Property",
            vec![Op::SetProperty { object: ObjectRef::Project, key: "after".into(), before: None, after: Some("v".into()) }],
        )
        .unwrap();
    assert!(o.ledger.state_at(1).is_err(), "nothing more is read through a halted file");
    let e = o.project.save(&mut o.ledger).expect_err("a read-only file does not save");
    assert!(!e.to_string().is_empty());
    drop(o);
}

/// Damage the golden file at random, as a disk or a sync tool might.
fn damaged(golden: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut b = golden.to_vec();
    let n = b.len() as u64;
    match rng.below(5) {
        0 | 1 => {
            for _ in 0..=rng.below(8) {
                let at = rng.below(n) as usize;
                b[at] ^= 1 << rng.below(8);
            }
        }
        2 => {
            let at = rng.below(n - 64) as usize;
            for byte in &mut b[at..at + 4 + rng.below(60) as usize] {
                *byte = rng.below(256) as u8;
            }
        }
        3 => {
            let at = rng.below(n - 4096) as usize;
            b[at..at + 4096].fill(0);
        }
        _ => b.truncate(rng.below(n) as usize),
    }
    b
}

#[test]
#[ignore = "slow: opens, reads back and verifies hundreds of damaged copies of the golden file"]
fn seeded_damage_to_the_golden_file_never_crashes_open_reads_or_verify() {
    let rounds: u64 = std::env::var("ECAD_DAMAGE_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let golden = std::fs::read(GOLDEN).unwrap();
    let dir = TempDir::new("damage-probe");
    let (mut opened, mut refused) = (0, 0);
    for round in 0..rounds {
        let seed = 0xDA4A_0000 + round;
        let bytes = damaged(&golden, &mut Rng::new(seed));
        let p = dir.path(&format!("r{round}.ecad"));
        std::fs::write(&p, &bytes).unwrap();
        // An escaped panic fails this test; the seed names the round.
        let run = std::panic::catch_unwind(|| match DiskProject::open(&p, SEED, WRITER) {
            Ok(o) => {
                for seq in [0, o.ledger.head() / 2] {
                    let _ = o.ledger.state_at(seq);
                }
                let _ = ecad_file::verify::verify(o.project.project().store());
                true
            }
            Err(_) => false,
        });
        match run {
            Ok(true) => opened += 1,
            Ok(false) => refused += 1,
            Err(_) => panic!("round {round} (seed {seed:#x}): a panic escaped"),
        }
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(p.with_extension("ecad.lock"));
    }
    eprintln!("{rounds} damaged copies: {opened} opened, {refused} refused");
}
