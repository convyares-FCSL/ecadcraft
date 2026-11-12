//! Crash safety of container version 2 (brief 021, EEP-0005 §12): permanent tests, in the slow
//! set (run at every QA gate with `--include-ignored`).
//!
//! - **Power loss** ([`ecad_store::power`]): a save is cut at a random point of its writes and
//!   syncs; the disk then holds the durable image plus any subset of the unsynced writes, in any
//!   order, each possibly torn at a sector. Reopening must show exactly the last acknowledged
//!   save, or the cut save complete; never a partial save, never less than what was acknowledged
//!   (the bake-off's own-container bug rolled back past good saves).
//! - **Torn write**: every unsynced write lands but the last is cut at an arbitrary byte.
//! - **Process kill**: a child process saves in a loop (single edits, 100-edit saves, versions,
//!   assets) and is killed at a random moment; reopening shows every acknowledged save, version
//!   and asset, and the whole file verifies. Also measures the reopen time after the kill.
//!
//! Round counts: `ECAD_CRASH_ROUNDS` (default 300 power-loss and torn-write rounds, 100 kill
//! rounds). Every failure prints its round's seed.
#![cfg(not(target_arch = "wasm32"))]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::common::{PROJECT, fixture};
use ecad_file::v2::schema::VersionKind;
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::*;
use ecad_store::power::{CrashMode, PowerLoss, Rng};

fn rounds(default: u64) -> u64 {
    std::env::var("ECAD_CRASH_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// `n` edits on the first page: lines, sometimes an undo, sometimes a named version.
fn edit(ledger: &mut Ledger, rng: &mut Rng, n: u64, version: bool) {
    let doc = ledger.document();
    let page = doc.page_order[0];
    let layer = *doc.layers.keys().next().unwrap();
    for i in 0..n {
        let id = ItemId(u128::from(rng.next_u64()) << 64 | u128::from(rng.next_u64()));
        let x = (rng.below(1000) as f64) / 10.0;
        let item = Item {
            page,
            style: Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 },
            kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(x, 5.0 + i as f64) }),
        };
        ledger.commit(Author::User, "LINE", vec![Op::AddItem { id, item }]).unwrap();
        if rng.below(10) == 0 {
            ledger.undo(Author::User).unwrap();
        }
    }
    if version {
        let at = ledger.head();
        let name = format!("V{at}");
        ledger.commit(Author::User, "Name", vec![Op::NameVersion { name, at, description: None }]).unwrap();
    }
}

/// One power-loss round; the description of a failure. With `span` `None` nothing is cut and
/// the number of disk operations of the last save is returned; with `Some(ops)` the power is cut
/// after a uniformly chosen number of operations up to the end of that save (the round is
/// deterministic, so the save is the same).
fn power_round(seed: u64, mode: CrashMode, span: Option<u64>) -> std::result::Result<(&'static str, u64), String> {
    let mut rng = Rng::new(seed);
    let disk = PowerLoss::default();
    let mut f = fixture();
    let store = Store::with_backend(disk.clone()).map_err(|e| e.to_string())?;
    let (mut proj, _) = ProjectV2::create(store, PROJECT, &f.ledger, None, None, None, "crash", 16).map_err(|e| e.to_string())?;
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    let mut ledger = f.ledger;
    let mut acked = (ledger.head(), ledger.document().clone());
    for _ in 0..rng.below(4) {
        let n = 1 + rng.below(20);
        let v = rng.below(5) == 0;
        edit(&mut ledger, &mut rng, n, v);
        proj.save(&mut ledger).map_err(|e| format!("a save before the cut failed: {e}"))?;
        acked = (ledger.head(), ledger.document().clone());
    }
    let n = 1 + rng.below(40);
    let v = rng.below(4) == 0;
    edit(&mut ledger, &mut rng, n, v);
    let attempted = (ledger.head(), ledger.document().clone());
    let pick = rng.next_u64();
    if let Some(ops) = span {
        disk.cut_after(pick % (ops + 1));
    }
    let before = disk.ops();
    let saved = proj.save(&mut ledger);
    let ops = disk.ops() - before;
    if saved.is_ok() {
        acked = attempted.clone();
    }
    drop(proj);
    let image = disk.crash(&mut rng, mode);
    let store = Store::with_backend(PowerLoss::from_image(image)).map_err(|e| format!("reopen: {e}"))?;
    let o = ProjectV2::open(store, 1, "crash").map_err(|e| format!("open: {e}"))?;
    if let Some(d) = &o.report.damage {
        return Err(format!("damage after the cut: {d:?}"));
    }
    let head = o.ledger.head();
    let (expected, outcome) = if head == acked.0 {
        (&acked.1, if saved.is_ok() { "saved before the cut" } else { "cut: reopened at the last acknowledged save" })
    } else if head == attempted.0 && saved.is_err() {
        (&attempted.1, "cut: the save had landed complete")
    } else {
        return Err(format!(
            "reopened at {head}; acknowledged {}, attempted {} ({})",
            acked.0,
            attempted.0,
            if saved.is_ok() { "acknowledged" } else { "cut" }
        ));
    };
    if o.ledger.document() != expected {
        return Err(format!("the document at {head} is not the saved one"));
    }
    let v = ecad_file::v2::verify::verify(o.project.store());
    if !v.ok {
        return Err(format!("verify: {:?}", v.lines));
    }
    Ok((outcome, ops))
}

fn run_rounds(name: &str, n: u64, mode: CrashMode) {
    let started = Instant::now();
    let mut failures = Vec::new();
    let mut outcomes: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for round in 0..n {
        let seed = 0xEC4D_0000 + round;
        // The same round twice: once to count the save's disk operations, then cut anywhere in it.
        let cut = power_round(seed, mode, None).and_then(|(_, ops)| power_round(seed, mode, Some(ops)));
        match cut {
            Ok((o, _)) => *outcomes.entry(o).or_default() += 1,
            Err(e) => failures.push(format!("round {round} (seed {seed:#x}): {e}")),
        }
    }
    eprintln!("{name}: {n} rounds, {} failures, {:.1} s; {outcomes:?}", failures.len(), started.elapsed().as_secs_f64());
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn power_loss_mid_save_keeps_every_acknowledged_save_quick() {
    run_rounds("power loss (quick)", 12, CrashMode::Reordered);
}

#[test]
#[ignore = "slow: hundreds of simulated power cuts"]
fn power_loss_mid_save_never_loses_an_acknowledged_save() {
    run_rounds("power loss", rounds(300), CrashMode::Reordered);
}

#[test]
#[ignore = "slow: hundreds of torn last writes"]
fn a_torn_last_write_never_loses_an_acknowledged_save() {
    run_rounds("torn write", rounds(300), CrashMode::TornLast);
}

// --- Process kill -----------------------------------------------------------------------------

const CHILD_FILE: &str = "ECAD_CRASH_FILE";

/// Acknowledgements: one line per committed save (`s <head>`), version (`v <name> <seq>`) or
/// asset (`a <hash>`), written only after the commit returned.
fn acks_path(file: &Path) -> PathBuf {
    file.with_extension("acks")
}

#[test]
#[ignore = "run by killed_mid_save_reopens_at_the_last_acknowledged_save in a child process"]
fn child_v2_writer() {
    let Some(file) = std::env::var_os("ECAD_TEST_CHILD").and(std::env::var_os(CHILD_FILE)) else { return };
    let file = PathBuf::from(file);
    let (mut disk, mut ledger) = if file.exists() {
        let o = ecad_file::v2::disk::DiskProjectV2::open(&file, std::process::id().into(), "crash child").unwrap();
        assert!(o.report.damage.is_none(), "{:?}", o.report);
        (o.project, o.ledger)
    } else {
        let mut f = fixture();
        let (d, _) = ecad_file::v2::disk::DiskProjectV2::create(&file, PROJECT, &mut f.ledger, None, None, "crash child").unwrap();
        (d, f.ledger)
    };
    let mut acks = std::fs::OpenOptions::new().create(true).append(true).open(acks_path(&file)).unwrap();
    let mut rng = Rng::new(u64::from(std::process::id()) ^ 0x5EED);
    ack(&mut acks, &format!("s {} {}", ledger.head(), digest(ledger.document())));
    loop {
        let r = rng.below(100);
        if r == 0 {
            // A 500 KB asset, in its own write transaction.
            let bytes: Vec<u8> = (0..500_000).map(|_| rng.next_u64() as u8).collect();
            let store = disk.project().store().clone();
            let mut w = store.writer().unwrap();
            let h = w.put_asset("application/octet-stream", &bytes).unwrap();
            w.commit().unwrap();
            ack(&mut acks, &format!("a {}", ecad_file::chain::hex(&h)));
            continue;
        }
        let (n, version) = match r {
            1..=5 => (1 + rng.below(5), true),
            6..=20 => (100, false),
            _ => (1, false),
        };
        edit(&mut ledger, &mut rng, n, version);
        disk.save(&mut ledger).unwrap();
        if version {
            let seq = ledger.head() - 1;
            ack(&mut acks, &format!("v V{seq} {seq}"));
        }
        ack(&mut acks, &format!("s {} {}", ledger.head(), digest(ledger.document())));
    }
}

/// A short fingerprint of a document's contents (the hash of its serde JSON).
fn digest(doc: &Document) -> String {
    ecad_file::chain::hex(&ecad_file::v2::schema::chunk_hash(&serde_json::to_vec(doc).unwrap()))
}

fn ack_lines(file: &Path) -> Vec<String> {
    std::fs::read_to_string(acks_path(file)).unwrap_or_default().lines().map(String::from).collect()
}

/// Appends one acknowledgement line in a single write. `writeln!` on a `File` writes each piece
/// of the format separately, so a kill between them left a torn line (`s 1316 ` and no
/// fingerprint) that the next child's first line was appended to.
fn ack(acks: &mut std::fs::File, line: &str) {
    acks.write_all(format!("{line}\n").as_bytes()).unwrap();
}

/// A whole fingerprint or asset hash (SHA-256, 64 hex digits), not one torn by a kill.
fn whole_hash(h: &str) -> bool {
    h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())
}

/// One acknowledgement line of the kill test.
enum Ack {
    Save(u64, String),
    Version(String, u64),
    Asset(String),
}

/// `line` as an acknowledgement, or `None` for a line a kill tore (the child died before the
/// acknowledgement was whole, so it acknowledged nothing).
fn parse_ack(line: &str) -> Option<Ack> {
    let mut it = line.split(' ');
    let ack = match (it.next()?, it.next()?) {
        ("s", head) => {
            let d = it.next()?;
            whole_hash(d).then_some(())?;
            Ack::Save(head.parse().ok()?, d.to_string())
        }
        ("v", name) => {
            let seq: u64 = it.next()?.parse().ok()?;
            (name == format!("V{seq}")).then_some(())?;
            Ack::Version(name.to_string(), seq)
        }
        ("a", h) => {
            whole_hash(h).then_some(())?;
            Ack::Asset(h.to_string())
        }
        _ => return None,
    };
    it.next().is_none().then_some(ack)
}

#[test]
#[ignore = "slow: kills a child process mid-save many times"]
fn killed_mid_save_reopens_at_the_last_acknowledged_save() {
    let dir = std::env::temp_dir().join(format!("ecad-crash-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("crash.ecad");
    let n = rounds(100) / 3 + 1;
    let n = if std::env::var("ECAD_CRASH_ROUNDS").is_ok() { rounds(100) } else { n.max(100) };
    let mut rng = Rng::new(0xC0FFEE);
    let mut failures = Vec::new();
    let mut reopen_ms = Vec::new();
    let mut acked_total = 0usize;
    let mut torn = 0usize;
    for round in 0..n {
        let before = ack_lines(&file).len();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "container_crash::child_v2_writer", "--nocapture", "--test-threads=1"])
            .env("ECAD_TEST_CHILD", "1")
            .env(CHILD_FILE, &file)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        // Wait until it is writing (its first acknowledgement), then kill it at a random moment.
        let start = Instant::now();
        while ack_lines(&file).len() <= before && start.elapsed() < Duration::from_secs(60) {
            if let Ok(Some(status)) = child.try_wait() {
                failures.push(format!("round {round}: the writer exited by itself ({status})"));
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::sleep(Duration::from_millis(rng.below(250)));
        let _ = child.kill();
        let _ = child.wait();
        let lines = ack_lines(&file);
        acked_total += lines.len().saturating_sub(before);
        let acks: Vec<Ack> = lines.iter().filter_map(|l| parse_ack(l)).collect();
        torn = lines.len() - acks.len();
        // Acknowledged saves: head → the document's fingerprint.
        let saves: std::collections::BTreeMap<u64, String> = acks
            .iter()
            .filter_map(|a| match a {
                Ack::Save(head, d) => Some((*head, d.clone())),
                _ => None,
            })
            .collect();
        let last_save = saves.keys().next_back().copied().unwrap_or(0);
        let t = Instant::now();
        let opened = ecad_file::v2::disk::DiskProjectV2::open(&file, 1, "crash parent");
        reopen_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        let o = match opened {
            Ok(o) => o,
            Err(e) => {
                failures.push(format!("round {round}: reopen failed: {e}"));
                break;
            }
        };
        if let Some(d) = &o.report.damage {
            failures.push(format!("round {round}: damage: {d:?}"));
        }
        if o.ledger.head() < last_save {
            failures.push(format!("round {round}: reopened at {}, but {last_save} was acknowledged", o.ledger.head()));
        }
        // The contents: the document an acknowledged save had, and every invariant.
        if let Some(d) = saves.get(&o.ledger.head())
            && *d != digest(o.ledger.document())
        {
            failures.push(format!("round {round}: the document at {} is not the one saved", o.ledger.head()));
        }
        if let Err(e) = o.ledger.document().check() {
            failures.push(format!("round {round}: the reopened document breaks an invariant: {e}"));
        }
        let store = o.project.project().store().clone();
        let r = store.reader().unwrap();
        let branch = o.project.project().branch();
        for a in &acks {
            match a {
                Ack::Version(name, seq) => match r.version(branch, VersionKind::User, name).unwrap() {
                    Some(v) if v.point.seq == *seq => {}
                    other => failures.push(format!("round {round}: acknowledged version {name} is {other:?}")),
                },
                Ack::Asset(h) => {
                    let hash = ecad_file::chain::parse_hex(h).unwrap();
                    if r.asset(&hash).unwrap().is_none() {
                        failures.push(format!("round {round}: acknowledged asset {h} is missing"));
                    }
                }
                Ack::Save(..) => {}
            }
        }
        drop(r);
        // The whole file every 10 rounds (it reads everything).
        if round % 10 == 9 || round + 1 == n {
            let v = ecad_file::v2::verify::verify(&store);
            if !v.ok {
                failures.push(format!("round {round}: verify: {:?}", v.lines));
            }
        }
        drop(store);
        drop(o);
        if !failures.is_empty() {
            break;
        }
    }
    reopen_ms.sort_by(f64::total_cmp);
    let pick = |q: f64| reopen_ms.get(((reopen_ms.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(0.0);
    let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    eprintln!(
        "process kill: {n} rounds, {acked_total} acknowledgements ({torn} torn by a kill, skipped), {} failures; reopen after kill median {:.1} ms, \
         p95 {:.1} ms, worst {:.1} ms; file {size} bytes",
        failures.len(),
        pick(0.5),
        pick(0.95),
        pick(1.0)
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "{failures:#?}");
}

const CHILD_DIR: &str = "ECAD_CRASH_DIR";

/// The Save As and first-save writer: in a folder of its own, it writes a new project's first
/// save, then alternates edits, saves, Save As to a new name and first saves of other new projects,
/// acknowledging each file written (`f <path> <head> <fingerprint>`) only after it returned.
#[test]
#[ignore = "run by killed_during_save_as_or_a_first_save_leaves_only_whole_files in a child process"]
fn child_v2_save_as_writer() {
    let Some(dir) = std::env::var_os("ECAD_TEST_CHILD").and(std::env::var_os(CHILD_DIR)) else { return };
    let dir = PathBuf::from(dir);
    let mut acks = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("acks")).unwrap();
    let mut rng = Rng::new(u64::from(std::process::id()) ^ 0x5A5A);
    let mut f = fixture();
    let first = dir.join("p0.ecad");
    let (mut disk, _) = ecad_file::v2::disk::DiskProjectV2::create(&first, PROJECT, &mut f.ledger, None, None, "crash child").unwrap();
    let mut ledger = f.ledger;
    ack(&mut acks, &format!("f {} {} {}", first.display(), ledger.head(), digest(ledger.document())));
    for i in 1..u32::MAX {
        let (n, v) = (1 + rng.below(30), rng.below(4) == 0);
        edit(&mut ledger, &mut rng, n, v);
        disk.save(&mut ledger).unwrap();
        ack(&mut acks, &format!("f {} {} {}", disk.path().display(), ledger.head(), digest(ledger.document())));
        if rng.below(2) == 0 {
            let target = dir.join(format!("p{i}.ecad"));
            disk.save_as(&target, &mut ledger, None, "crash child").unwrap();
            ack(&mut acks, &format!("f {} {} {}", target.display(), ledger.head(), digest(ledger.document())));
        } else {
            // The first save of another new project.
            let mut g = fixture();
            let n = 1 + rng.below(50);
            edit(&mut g.ledger, &mut rng, n, false);
            let target = dir.join(format!("n{i}.ecad"));
            let (d, _) = ecad_file::v2::disk::DiskProjectV2::create(&target, PROJECT, &mut g.ledger, None, None, "crash child").unwrap();
            drop(d);
            ack(&mut acks, &format!("f {} {} {}", target.display(), g.ledger.head(), digest(g.ledger.document())));
        }
    }
}

/// Killed during a Save As or a first save: every acknowledged file opens with what was saved in
/// it (or more), and every other project file in the folder is whole (the rename is atomic): no
/// half-written project at a project's name.
#[test]
#[ignore = "slow: kills a child process during Save As and first saves"]
fn killed_during_save_as_or_a_first_save_leaves_only_whole_files() {
    let n = rounds(50);
    let mut rng = Rng::new(0x5AFE);
    let mut failures = Vec::new();
    let mut files_checked = 0usize;
    for round in 0..n {
        let dir = std::env::temp_dir().join(format!("ecad-crash-as-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&dir).unwrap();
        let acks_file = dir.join("acks");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "container_crash::child_v2_save_as_writer", "--nocapture", "--test-threads=1"])
            .env("ECAD_TEST_CHILD", "1")
            .env(CHILD_DIR, &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while std::fs::read_to_string(&acks_file).unwrap_or_default().is_empty() && start.elapsed() < Duration::from_secs(60) {
            if let Ok(Some(status)) = child.try_wait() {
                failures.push(format!("round {round}: the writer exited by itself ({status})"));
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::sleep(Duration::from_millis(rng.below(400)));
        let _ = child.kill();
        let _ = child.wait();
        // The newest acknowledgement of each file.
        let mut acked: std::collections::BTreeMap<String, (u64, String)> = std::collections::BTreeMap::new();
        for l in std::fs::read_to_string(&acks_file).unwrap_or_default().lines() {
            let mut it = l.strip_prefix("f ").unwrap_or_default().rsplitn(3, ' ');
            let (Some(d), Some(Ok(h)), Some(p)) = (it.next(), it.next().map(str::parse), it.next()) else { continue };
            // A line torn by the kill acknowledged nothing.
            if whole_hash(d) {
                acked.insert(p.to_string(), (h, d.to_string()));
            }
        }
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("ecad") {
                continue;
            }
            files_checked += 1;
            let key = path.display().to_string();
            match ecad_file::v2::disk::DiskProjectV2::open(&path, 1, "crash parent") {
                Ok(o) => {
                    if let Some(d) = &o.report.damage {
                        failures.push(format!("round {round}: {key} is damaged: {d:?}"));
                    }
                    if let Some((head, fp)) = acked.get(&key) {
                        if o.ledger.head() < *head {
                            failures.push(format!("round {round}: {key} reopened at {}, {head} acknowledged", o.ledger.head()));
                        } else if o.ledger.head() == *head && *fp != digest(o.ledger.document()) {
                            failures.push(format!("round {round}: {key} does not hold what was saved"));
                        }
                    }
                }
                Err(e) => failures.push(format!("round {round}: {key} does not open: {e}")),
            }
        }
        for path in acked.keys() {
            if !Path::new(path).exists() {
                failures.push(format!("round {round}: acknowledged {path} is missing"));
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        if !failures.is_empty() {
            break;
        }
    }
    eprintln!("Save As and first saves killed: {n} rounds, {files_checked} files checked, {} failures", failures.len());
    assert!(failures.is_empty(), "{failures:#?}");
}
