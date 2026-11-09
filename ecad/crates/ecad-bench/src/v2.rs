//! Container version 2 timings (brief 021): the v2 columns of `cargo xtask bench`, and the
//! Zstandard comparison the overseer asked for (`ruzstd` against the `zstd` crate).
//!
//! # Semantics
//! - **convert**: the version 1 file converted to a new version 2 file, verified by replaying both.
//! - **v2 open**: [`DiskProjectV2::open`] from a warm file cache: the latest snapshot and its tail.
//! - **v2 save**: one add-wire transaction committed (not timed) and saved: one redb write
//!   transaction, durable before it returns.
//! - **v2 crash open**: a child process saves an edit and exits without closing the file (as a
//!   killed app leaves it); the open after it, including redb's quick repair.
//! - **v2 open ×10**: the open after the history has grown tenfold (property toggles saved in
//!   batches of 1 000, with the automatic snapshots that brings): it should stay within the run's
//!   noise of **v2 open** (EEP-0005 §5.6).

use std::path::Path;
use std::time::Instant;

use ecad_file::v2::disk::DiskProjectV2;
use ecad_model::{Author, IdAllocator, ObjectRef, Op};

use crate::measure::{HOLDER, median};

const SEED: u64 = 0x0002_BE4C;
const EDIT_SEED: u64 = 0xED17_0021;

/// What the version 2 file measured, times in milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredV2 {
    pub bytes: u64,
    pub convert_ms: f64,
    pub open_ms: f64,
    pub save_ms: f64,
    pub crash_open_ms: f64,
    pub open_x10_ms: f64,
    pub bytes_x10: u64,
    /// Bytes redb holds in rows (keys and values) after the ×10 growth.
    pub stored_x10: u64,
    /// The ×10 file's size after engine compaction (as Save As writes it).
    pub compacted_x10: u64,
    /// On the ×10 file: the first undo that reaches below the 1 024 stack entries the snapshot
    /// recorded, which reads the whole stacks from the history once (EEP-0005 §5.5).
    pub undo_past_cap_ms: f64,
    /// The undos before it, median.
    pub undo_ms: f64,
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn open(path: &Path) -> Result<ecad_file::v2::disk::OpenedDiskV2, String> {
    let o = DiskProjectV2::open(path, SEED, HOLDER).map_err(|e| e.to_string())?;
    if let Some(d) = &o.report.damage {
        return Err(format!("{} opened damaged: {d:?}", path.display()));
    }
    if let Some(r) = o.project.read_only() {
        return Err(format!("{} opened read-only: {r}", path.display()));
    }
    Ok(o)
}

/// Convert `v1` to the version 2 file `v2` (timed).
///
/// # Errors
/// The conversion failed.
pub fn convert(v1: &Path, v2: &Path) -> Result<f64, String> {
    crate::measure::remove(v2);
    let t = Instant::now();
    ecad_file::v2::convert::convert_file(v1, Some(v2), HOLDER)?;
    Ok(ms(t))
}

/// Measure the version 2 file at `path`, `runs` times each; `scratch` holds the copies the save,
/// crash and ×10 timings write. `convert_ms` is passed through.
///
/// # Errors
/// Opening, saving or the child process failed.
pub fn measure(path: &Path, runs: usize, scratch: &Path, convert_ms: f64) -> Result<MeasuredV2, String> {
    let bytes = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    let mut open_ms = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        let o = open(path)?;
        open_ms.push(ms(t));
        drop(o);
    }
    crate::measure::release_lock_file(path);

    // Save per edit.
    let copy = scratch.join("v2-save.ecad");
    copy_file(path, &copy)?;
    let mut save_ms = Vec::with_capacity(runs);
    {
        let mut o = open(&copy)?;
        let mut ids = IdAllocator::seeded(EDIT_SEED);
        for run in 0..runs.max(5) {
            let ops = crate::generate::added_wire(o.ledger.document(), &mut ids, &format!("W-v2-{}", run + 1))?;
            o.ledger.commit(Author::User, "Wire", ops).map_err(|e| e.to_string())?;
            let t = Instant::now();
            o.project.save(&mut o.ledger).map_err(|e| e.to_string())?;
            save_ms.push(ms(t));
        }
    }
    crate::measure::remove(&copy);

    // Open after a crash: a child saves and exits without closing the file.
    let copy = scratch.join("v2-crash.ecad");
    copy_file(path, &copy)?;
    let mut crash_ms = Vec::with_capacity(runs);
    for _ in 0..runs {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let status = std::process::Command::new(exe).arg("--crash-save").arg(&copy).status().map_err(|e| format!("starting the crash child: {e}"))?;
        if !status.success() {
            return Err(format!("the crash child failed ({status})"));
        }
        let t = Instant::now();
        let o = DiskProjectV2::open(&copy, SEED, HOLDER).map_err(|e| e.to_string())?;
        crash_ms.push(ms(t));
        if !o.report.repaired && runs > 0 {
            eprintln!("note: the file did not need a repair after the child's exit");
        }
        drop(o);
    }
    crate::measure::remove(&copy);

    // Open after the history grew tenfold.
    let copy = scratch.join("v2-x10.ecad");
    copy_file(path, &copy)?;
    {
        let mut o = open(&copy)?;
        let start = o.ledger.head();
        let target = start * 10;
        let mut flip = false;
        while o.ledger.head() < target {
            for _ in 0..1_000.min(target - o.ledger.head()) {
                let (before, after) = if flip {
                    (Some("b".to_string()), Some("a".to_string()))
                } else {
                    let cur = o.ledger.document().properties.get("bench.x10").cloned();
                    (cur, Some("b".to_string()))
                };
                flip = after.as_deref() == Some("b");
                let ops = vec![Op::SetProperty { object: ObjectRef::Project, key: "bench.x10".into(), before, after }];
                o.ledger.commit(Author::User, "x10", ops).map_err(|e| e.to_string())?;
            }
            o.project.save(&mut o.ledger).map_err(|e| e.to_string())?;
        }
    }
    crate::measure::release_lock_file(&copy);
    let bytes_x10 = std::fs::metadata(&copy).map_or(0, |m| m.len());
    let mut x10_ms = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        let o = open(&copy)?;
        x10_ms.push(ms(t));
        drop(o);
    }
    // Undo past the recorded stack top: 1 024 undos read nothing; the next one reads the history
    // once.
    let (undo_past_cap_ms, undo_ms) = {
        let mut o = open(&copy)?;
        // Undo until a step reads the whole stacks (the ones the snapshot recorded, 1 024 at most,
        // and the tail's are in memory): time that step, and the others.
        let held = o.ledger.stacks().0.len();
        let mut each = Vec::with_capacity(held + 1);
        let mut past = None;
        for _ in 0..=held {
            let before = o.ledger.stack_rebuilds();
            let t = Instant::now();
            o.ledger.undo(Author::User).map_err(|e| e.to_string())?;
            let took = ms(t);
            if o.ledger.stack_rebuilds() > before {
                past = Some(took);
                break;
            }
            each.push(took);
        }
        let past = past.ok_or("no undo reached below the recorded stack top")?;
        (past, median(&mut each))
    };
    crate::measure::release_lock_file(&copy);
    let stored_x10 = ecad_file::v2::Store::open(&copy, false)
        .and_then(|s| s.reader())
        .and_then(|r| r.stats())
        .map(|t| t.iter().map(|t| t.stored_bytes).sum())
        .map_err(|e| e.to_string())?;
    let compacted_x10 = {
        let mut s = ecad_file::v2::Store::open(&copy, true).map_err(|e| e.to_string())?;
        s.compact().map_err(|e| e.to_string())?;
        drop(s);
        std::fs::metadata(&copy).map_or(0, |m| m.len())
    };
    crate::measure::remove(&copy);

    Ok(MeasuredV2 {
        bytes,
        convert_ms,
        open_ms: median(&mut open_ms),
        save_ms: median(&mut save_ms),
        crash_open_ms: median(&mut crash_ms),
        open_x10_ms: median(&mut x10_ms),
        bytes_x10,
        stored_x10,
        compacted_x10,
        undo_past_cap_ms,
        undo_ms,
    })
}

fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    crate::measure::remove(to);
    std::fs::copy(from, to).map(|_| ()).map_err(|e| format!("copying {} to {}: {e}", from.display(), to.display()))
}

/// The crash child: open `path`, save one edit, and exit without closing anything (no
/// destructors run), as a killed app leaves the file.
///
/// # Errors
/// Opening or saving failed.
pub fn crash_save(path: &Path) -> Result<(), String> {
    let mut o = open(path)?;
    let mut ids = IdAllocator::seeded(EDIT_SEED ^ u64::from(std::process::id()));
    let ops = crate::generate::added_wire(o.ledger.document(), &mut ids, "W-crash")?;
    o.ledger.commit(Author::User, "Wire", ops).map_err(|e| e.to_string())?;
    o.project.save(&mut o.ledger).map_err(|e| e.to_string())?;
    std::process::exit(0)
}

/// Zstandard, compared (overseer, 2026-11-08): every transaction record and chunk of a version 2
/// file, decompressed, then compressed by each codec. Sizes in bytes, times in milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Codec {
    pub name: &'static str,
    pub rows_bytes: u64,
    pub parts_bytes: u64,
    pub compress_ms: f64,
    pub decompress_ms: f64,
}

/// Raw bytes of the rows and chunks, and the comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Compression {
    pub raw_rows: u64,
    pub raw_chunks: u64,
    pub codecs: Vec<Codec>,
}

/// Compare `ruzstd` (level Fastest, the only one it implements) with the `zstd` crate at levels 1
/// and 3 on the rows and chunks of the version 2 file at `path`.
///
/// # Errors
/// The file cannot be read, or a codec failed.
pub fn compression(path: &Path) -> Result<Compression, String> {
    let store = ecad_file::v2::Store::open(path, false).map_err(|e| e.to_string())?;
    let r = store.reader().map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    let mut chunks = Vec::new();
    r.raw_rows(|table, _, v| {
        match table {
            "transactions" => {
                let (flag, _, body) = ecad_file::v2::schema::split_tx_value(v)?;
                rows.push(ecad_file::v2::pack::unpack(flag, body)?);
            }
            "chunks" => {
                let flag = *v.first().ok_or("empty chunk")?;
                chunks.push(ecad_file::v2::pack::unpack(flag, v.get(1..).unwrap_or_default())?);
            }
            _ => {}
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    drop(r);
    drop(store);
    let raw_rows = rows.iter().map(|b| b.len() as u64).sum();
    let raw_chunks = chunks.iter().map(|b| b.len() as u64).sum();
    let mut codecs = Vec::new();
    type Enc = fn(&[u8]) -> Result<Vec<u8>, String>;
    type Dec = fn(&[u8]) -> Result<Vec<u8>, String>;
    let all: [(&'static str, Enc, Dec); 3] = [
        (
            "ruzstd (Fastest)",
            |b| Ok(ruzstd::encoding::compress_to_vec(b, ruzstd::encoding::CompressionLevel::Fastest)),
            |b| ecad_file::v2::pack::decode_ruzstd(b),
        ),
        ("zstd 1", |b| zstd::bulk::compress(b, 1).map_err(|e| e.to_string()), |b| ecad_file::v2::pack::unpack(ecad_file::v2::pack::ZSTD, b)),
        ("zstd 3", |b| zstd::bulk::compress(b, 3).map_err(|e| e.to_string()), |b| ecad_file::v2::pack::unpack(ecad_file::v2::pack::ZSTD, b)),
    ];
    for (name, enc, dec) in all {
        let t = Instant::now();
        let z_rows: Vec<Vec<u8>> = rows.iter().map(|b| enc(b)).collect::<Result<_, _>>()?;
        let z_chunks: Vec<Vec<u8>> = chunks.iter().map(|b| enc(b)).collect::<Result<_, _>>()?;
        let compress_ms = ms(t);
        let t = Instant::now();
        for (z, raw) in z_rows.iter().zip(&rows).chain(z_chunks.iter().zip(&chunks)) {
            if dec(z)? != *raw {
                return Err(format!("{name} did not round-trip"));
            }
        }
        let decompress_ms = ms(t);
        // Stored as the container stores them: compressed only when smaller.
        let size = |z: &[Vec<u8>], raw: &[Vec<u8>]| z.iter().zip(raw).map(|(z, r)| z.len().min(r.len()) as u64).sum();
        codecs.push(Codec { name, rows_bytes: size(&z_rows, &rows), parts_bytes: size(&z_chunks, &chunks), compress_ms, decompress_ms });
    }
    Ok(Compression { raw_rows, raw_chunks, codecs })
}

/// A growth probe (developer use, `ecad-bench --growth <file.v2.ecad> <batches>`): append
/// `batches` saves of 1 000 property changes to a copy of `path`, printing the file size, the
/// bytes each table stores and the size after compaction every tenth batch.
///
/// # Errors
/// Opening, saving or reading failed.
pub fn growth(path: &Path, batches: u64) -> Result<(), String> {
    let copy = path.with_extension("growth.ecad");
    copy_file(path, &copy)?;
    let mut o = open(&copy)?;
    let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
    let mut flip = false;
    for b in 1..=batches {
        for _ in 0..1_000 {
            let before = o.ledger.document().properties.get("bench.growth").cloned();
            let after = if flip { "a" } else { "b" };
            flip = !flip;
            let ops = vec![Op::SetProperty { object: ObjectRef::Project, key: "bench.growth".into(), before, after: Some(after.into()) }];
            o.ledger.commit(Author::User, "growth", ops).map_err(|e| e.to_string())?;
        }
        let t = Instant::now();
        let s = o.project.save(&mut o.ledger).map_err(|e| e.to_string())?;
        if b % 10 == 0 || b == batches {
            let r = o.project.project().store().reader().map_err(|e| e.to_string())?;
            let stats = r.stats().map_err(|e| e.to_string())?;
            let tables: Vec<String> = stats
                .iter()
                .map(|t| {
                    format!(
                        "{} {} rows {:.1} MB (+{:.1} meta, {:.1} frag)",
                        t.name,
                        t.rows,
                        mb(t.stored_bytes),
                        mb(t.metadata_bytes),
                        mb(t.fragmented_bytes)
                    )
                })
                .collect();
            eprintln!(
                "batch {b}: head {}, save {:.1} ms ({} snapshots), file {:.1} MB; {}",
                o.ledger.head(),
                ms(t),
                s.snapshots,
                mb(size(&copy)),
                tables.join("; ")
            );
        }
    }
    drop(o);
    let mut st = ecad_file::v2::Store::open(&copy, true).map_err(|e| e.to_string())?;
    st.compact().map_err(|e| e.to_string())?;
    drop(st);
    eprintln!("compacted: {:.1} MB", mb(size(&copy)));
    crate::measure::remove(&copy);
    Ok(())
}

fn mb(b: u64) -> f64 {
    b as f64 / 1_048_576.0
}

/// A probe (developer use, `ecad-bench --probe <file.v2.ecad>`): open a copy three times, then
/// save ten one-wire edits, printing each time.
///
/// # Errors
/// Opening or saving failed.
pub fn probe(path: &Path) -> Result<(), String> {
    let copy = path.with_extension("probe.ecad");
    copy_file(path, &copy)?;
    for _ in 0..3 {
        let t = Instant::now();
        let o = open(&copy)?;
        eprintln!("open {:.1} ms (base {}, tail {})", ms(t), o.ledger.base(), o.ledger.log().len());
        drop(o);
    }
    let mut o = open(&copy)?;
    let mut ids = IdAllocator::seeded(EDIT_SEED ^ 7);
    for i in 0..10 {
        let ops = crate::generate::added_wire(o.ledger.document(), &mut ids, &format!("W-probe-{i}"))?;
        o.ledger.commit(Author::User, "Wire", ops).map_err(|e| e.to_string())?;
        let t = Instant::now();
        o.project.save(&mut o.ledger).map_err(|e| e.to_string())?;
        eprintln!("save {:.2} ms", ms(t));
    }
    drop(o);
    crate::measure::remove(&copy);
    Ok(())
}
