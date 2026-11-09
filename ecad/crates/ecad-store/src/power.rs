//! A storage backend for the version 2 container (a redb database) that simulates power loss
//! (EEP-0005 §12.1): what a process kill cannot reproduce.
//!
//! Every write and length change is applied to the current image and remembered as pending until
//! the next `sync_data`, which makes the image durable. [`PowerLoss::crash`] then returns what a
//! disk could hold after power was cut at that moment: the durable image plus any subset of the
//! pending writes, **in any order**, each possibly **torn** (only a prefix written). Writes never
//! cross a sync: that is the ordering a flush guarantees, and the only one.
//!
//! [`PowerLoss::cut_after`] arms a cut: after that many more writes, length changes and syncs,
//! every operation fails, as if the machine stopped, so a commit in progress never completes.
//!
//! Excluded: anything about real disks beyond that model (lying flushes, damage at rest).

use std::io;
use std::ops::Bound;
use std::sync::{Arc, Mutex, MutexGuard};

/// One pending change, since the last sync.
#[derive(Debug, Clone)]
enum Change {
    Write { offset: u64, data: Vec<u8> },
    SetLen(u64),
}

#[derive(Debug, Default)]
struct Inner {
    current: Vec<u8>,
    durable: Vec<u8>,
    pending: Vec<Change>,
    /// Operations left before the cut; `None`: not armed.
    left: Option<u64>,
    /// The power is off: every operation fails.
    off: bool,
    syncs: u64,
    writes: u64,
    /// Writes, length changes and syncs so far (the unit [`PowerLoss::cut_after`] counts).
    ops: u64,
}

/// A small deterministic random source (xorshift64*), so a failing round can be replayed from
/// its seed.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number below `n` (0 when `n` is 0).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }
}

/// A shared handle to the simulated disk; clones see the same image.
#[derive(Debug, Clone, Default)]
pub struct PowerLoss {
    inner: Arc<Mutex<Inner>>,
}

/// How [`PowerLoss::crash`] treats the pending writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashMode {
    /// Any subset, in any order, each possibly torn at a 512-byte sector boundary.
    Reordered,
    /// All pending writes in order, the last one torn at an arbitrary byte.
    TornLast,
}

fn lock(m: &Mutex<Inner>) -> io::Result<MutexGuard<'_, Inner>> {
    m.lock().map_err(|_| io::Error::other("the simulated disk's lock is poisoned"))
}

fn apply(image: &mut Vec<u8>, c: &Change, limit: Option<usize>) {
    match c {
        Change::Write { offset, data } => {
            let n = limit.unwrap_or(data.len()).min(data.len());
            let Ok(start) = usize::try_from(*offset) else { return };
            let end = start.saturating_add(n);
            if image.len() < end {
                image.resize(end, 0);
            }
            if let (Some(dst), Some(src)) = (image.get_mut(start..end), data.get(..n)) {
                dst.copy_from_slice(src);
            }
        }
        Change::SetLen(len) => {
            if let Ok(l) = usize::try_from(*len) {
                image.resize(l, 0);
            }
        }
    }
}

impl PowerLoss {
    /// A disk holding `image` (durable).
    pub fn from_image(image: Vec<u8>) -> Self {
        let inner = Inner { current: image.clone(), durable: image, ..Inner::default() };
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    /// Cut the power after `ops` more operations (writes, length changes, syncs).
    pub fn cut_after(&self, ops: u64) {
        if let Ok(mut i) = self.inner.lock() {
            i.left = Some(ops);
        }
    }

    /// Writes and syncs so far.
    pub fn counts(&self) -> (u64, u64) {
        self.inner.lock().map_or((0, 0), |i| (i.writes, i.syncs))
    }

    /// Writes, length changes and syncs so far: what [`PowerLoss::cut_after`] counts.
    pub fn ops(&self) -> u64 {
        self.inner.lock().map_or(0, |i| i.ops)
    }

    /// Whether the power has been cut.
    pub fn is_off(&self) -> bool {
        self.inner.lock().is_ok_and(|i| i.off)
    }

    /// What the disk could hold if the power were cut now (see [`CrashMode`]). The simulated
    /// disk itself is unchanged.
    pub fn crash(&self, rng: &mut Rng, mode: CrashMode) -> Vec<u8> {
        let Ok(i) = self.inner.lock() else { return Vec::new() };
        let mut image = i.durable.clone();
        match mode {
            CrashMode::Reordered => {
                let mut chosen: Vec<&Change> = i.pending.iter().filter(|_| rng.below(2) == 0).collect();
                // Shuffle (Fisher–Yates).
                for k in (1..chosen.len()).rev() {
                    let j = usize::try_from(rng.below(k as u64 + 1)).unwrap_or(0);
                    chosen.swap(k, j);
                }
                for c in chosen {
                    let limit = match c {
                        Change::Write { data, .. } if rng.below(4) == 0 => {
                            let sectors = data.len().div_ceil(512) as u64;
                            Some(usize::try_from(rng.below(sectors)).unwrap_or(0) * 512)
                        }
                        _ => None,
                    };
                    apply(&mut image, c, limit);
                }
            }
            CrashMode::TornLast => {
                let n = i.pending.len();
                for (k, c) in i.pending.iter().enumerate() {
                    let limit = match c {
                        Change::Write { data, .. } if k + 1 == n => Some(usize::try_from(rng.below(data.len() as u64 + 1)).unwrap_or(0)),
                        _ => None,
                    };
                    apply(&mut image, c, limit);
                }
            }
        }
        image
    }

    /// Count one operation against an armed cut; whether the power is (now) off.
    fn tick(i: &mut Inner) -> bool {
        i.ops += 1;
        if i.off {
            return true;
        }
        match i.left {
            Some(0) => {
                i.off = true;
                true
            }
            Some(n) => {
                i.left = Some(n - 1);
                false
            }
            None => false,
        }
    }
}

fn power_off() -> io::Error {
    io::Error::other("simulated power loss")
}

impl redb::StorageBackend for PowerLoss {
    fn len(&self) -> io::Result<u64> {
        Ok(lock(&self.inner)?.current.len() as u64)
    }

    fn read(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        let i = lock(&self.inner)?;
        let start = usize::try_from(offset).map_err(io::Error::other)?;
        let src =
            i.current.get(start..start.saturating_add(out.len())).ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "read past the end"))?;
        out.copy_from_slice(src);
        Ok(())
    }

    fn set_len(&self, len: u64) -> io::Result<()> {
        let mut i = lock(&self.inner)?;
        if Self::tick(&mut i) {
            return Err(power_off());
        }
        let c = Change::SetLen(len);
        apply(&mut i.current, &c, None);
        i.pending.push(c);
        Ok(())
    }

    fn sync_data(&self) -> io::Result<()> {
        let mut i = lock(&self.inner)?;
        if Self::tick(&mut i) {
            return Err(power_off());
        }
        i.durable = i.current.clone();
        i.pending.clear();
        i.syncs += 1;
        Ok(())
    }

    // One process uses the simulated disk: every byte-range lock is granted (redb's
    // `SingleWriter` mode, which the version 2 container uses, asks for them).
    fn try_lock_range(&self, _: Bound<u64>, _: Bound<u64>) -> Result<bool, redb::BackendError> {
        Ok(true)
    }

    fn try_lock_shared_range(&self, _: Bound<u64>, _: Bound<u64>) -> Result<bool, redb::BackendError> {
        Ok(true)
    }

    fn lock_range(&self, _: Bound<u64>, _: Bound<u64>) -> Result<(), redb::BackendError> {
        Ok(())
    }

    fn lock_shared_range(&self, _: Bound<u64>, _: Bound<u64>) -> Result<(), redb::BackendError> {
        Ok(())
    }

    fn unlock_range(&self, _: Bound<u64>, _: Bound<u64>) -> Result<(), redb::BackendError> {
        Ok(())
    }

    fn query_lock_range(&self, _: Bound<u64>, _: Bound<u64>) -> Result<bool, redb::BackendError> {
        Ok(false)
    }

    fn write(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        let mut i = lock(&self.inner)?;
        if Self::tick(&mut i) {
            return Err(power_off());
        }
        let c = Change::Write { offset, data: data.to_vec() };
        apply(&mut i.current, &c, None);
        i.pending.push(c);
        i.writes += 1;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use redb::StorageBackend;

    #[test]
    fn only_synced_writes_survive_for_certain_and_the_cut_stops_everything() {
        let d = PowerLoss::default();
        d.write(0, b"aaaa").unwrap();
        d.sync_data().unwrap();
        d.write(0, b"bb").unwrap();
        let mut rng = Rng::new(1);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..64 {
            seen.insert(d.crash(&mut rng, CrashMode::Reordered));
        }
        assert!(seen.contains(b"aaaa".as_slice()) && seen.contains(b"bbaa".as_slice()), "{seen:?}");
        assert!(seen.iter().all(|img| img.ends_with(b"aa")), "synced bytes are never lost");
        d.cut_after(1);
        d.write(4, b"c").unwrap();
        assert!(d.write(5, b"d").is_err() && d.sync_data().is_err() && d.is_off());
    }
}
