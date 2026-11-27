//! ecad-store: the storage project files live in, and nothing else.
//!
//! What is stored, which tables exist and how records are checked belong to `ecad-file`.
//!
//! - [`db`]: [`db::Database`], a redb database as generic storage: open (new, existing, read-only, over any
//!   backend, or from bytes), a read view, a write transaction with quick-repair, and reopening once after an
//!   I/O error.
//! - [`disk`] (native only): [`WriterLock`], the operating-system
//!   lock that makes one session the writer; and the steps of an atomic replacement (temporary file, rename,
//!   folder flush). It is also where `ecad-file` reaches the disk and the clock for everything else, because
//!   only this crate touches the OS: whole and partial reads, file lengths, path comparison, the lock holder
//!   record.
//! - [`guard`]: [`Halt`], which catches a panic inside redb (a damaged page can cause one), turns it into an
//!   error and halts the handle, so nothing more is read through it.
//! - [`random_u128`]: the operating system's randomness, for new IDs.
//! - [`backend`]: [`MemoryBackend`], a redb storage backend in memory, shared by its clones, with an injectable
//!   write failure (tests keep a real project file without a disk).
//! - [`power`]: a redb storage backend that simulates power loss (reordered and torn writes since the last
//!   flush), for the project file's crash tests.
//!
//! Excluded: what is written and in which order (`ecad-file`), and any file format: no table, record or `.ecad`
//! type is known here.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub(crate) mod backend;
pub mod db;
pub mod disk;
pub mod guard;
pub mod power;
pub use backend::MemoryBackend;
pub use disk::{FileIdentity, LockAttempt, WriterLock};
pub use guard::{Halt, Halted};

/// A random 128-bit value from the operating system's randomness (through the standard library's
/// hasher keys) and the clock: the entropy for new IDs such as `ecad-file`'s branch IDs, here because
/// only this crate touches the OS. Built on every target.
pub fn random_u128() -> u128 {
    use std::hash::{BuildHasher, Hasher};
    let mut id = [0u8; 16];
    for (i, half) in id.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_usize(i);
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        h.write_u128(t);
        half.copy_from_slice(&h.finish().to_be_bytes());
    }
    u128::from_be_bytes(id)
}
