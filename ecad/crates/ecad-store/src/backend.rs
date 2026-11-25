//! [`MemoryBackend`]: a redb storage backend held in memory, shared by its clones, with an
//! injectable write failure. Tests use it to keep a real project file without a disk.
//!
//! Excluded: anything a disk adds (locks between processes, power loss: see [`crate::power`]).

use std::io;
use std::ops::Bound;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Default)]
struct Inner {
    bytes: Vec<u8>,
    /// Writes and length changes left to fail.
    fail: u32,
}

/// A database's bytes in memory. Clones share the bytes: a store holds one clone, the test that
/// made it another, to read the bytes or inject a failure.
#[derive(Debug, Clone, Default)]
pub struct MemoryBackend(Arc<Mutex<Inner>>);

impl MemoryBackend {
    /// An empty backend (a new database).
    pub fn new() -> Self {
        Self::default()
    }

    /// A backend holding `bytes` (a copy of a database).
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        MemoryBackend(Arc::new(Mutex::new(Inner { bytes, fail: 0 })))
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves plain bytes behind: still usable.
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A copy of the bytes.
    pub fn bytes(&self) -> Vec<u8> {
        self.inner().bytes.clone()
    }

    /// The length of the bytes.
    pub fn byte_len(&self) -> usize {
        self.inner().bytes.len()
    }

    /// Make the next write (or length change) fail, once: a commit in progress fails with an I/O
    /// error and nothing it wrote is acknowledged.
    pub fn fail_next_write(&self) {
        self.inner().fail = 1;
    }

    /// Forget every byte (a new, empty database).
    pub fn clear(&self) {
        self.inner().bytes.clear();
    }

    fn tick(&self) -> io::Result<()> {
        let mut i = self.inner();
        if i.fail > 0 {
            i.fail -= 1;
            return Err(io::Error::other("injected write failure"));
        }
        Ok(())
    }
}

impl redb::StorageBackend for MemoryBackend {
    fn len(&self) -> io::Result<u64> {
        Ok(self.inner().bytes.len() as u64)
    }

    fn read(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        let i = self.inner();
        let start = usize::try_from(offset).map_err(io::Error::other)?;
        let src = start.checked_add(out.len()).and_then(|end| i.bytes.get(start..end));
        out.copy_from_slice(src.ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?);
        Ok(())
    }

    fn set_len(&self, len: u64) -> io::Result<()> {
        self.tick()?;
        let len = usize::try_from(len).map_err(io::Error::other)?;
        self.inner().bytes.resize(len, 0);
        Ok(())
    }

    fn sync_data(&self) -> io::Result<()> {
        Ok(())
    }

    fn write(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.tick()?;
        let mut i = self.inner();
        let start = usize::try_from(offset).map_err(io::Error::other)?;
        let end = start.checked_add(data.len()).ok_or_else(|| io::Error::other("write past the end of memory"))?;
        if i.bytes.len() < end {
            i.bytes.resize(end, 0);
        }
        if let Some(dst) = i.bytes.get_mut(start..end) {
            dst.copy_from_slice(data);
        }
        Ok(())
    }

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
}
