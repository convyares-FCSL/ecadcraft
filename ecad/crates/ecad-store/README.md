# ecad-store

Layer: Core

## Purpose
The storage under the project file: the redb database handle, and the operating-system pieces
that go with it. It is the one Core crate allowed to touch the OS (disk, lock, clock,
randomness). It does not know what the bytes mean; that is `ecad-file`.

## Contents
- `db`: `Database`, a redb database as generic storage: open (new, existing, read-only, over a backend, from bytes), read and write transactions, and reopening once after an I/O error. It knows no tables.
- `disk`: `FileIdentity` (which file a path names), `WriterLock` (makes one session the writer), atomic replacement (temporary file, rename, folder flush), and the disk and clock reads that `ecad-file` needs.
- `random_u128`: the system's randomness, for new ids.
- `MemoryBackend`: a redb storage backend in memory, shared by its clones, that can be made to fail once (tests keep a real project file without a disk).
- `dev::power` (feature `fault-injection`, which only tests and dev tools enable): a storage backend that simulates power loss, for the container's crash tests.

## Must not depend on
Core rules: no `cadcraft-*`, no UI crate, nothing above Core. In Core it is the only crate where
`std::fs`, `std::env`, `std::process`, clocks and randomness are allowed. It holds no file format
and does not decide the order of write steps.

## Key types
`db::Database`, `MemoryBackend`, `FileIdentity`, `WriterLock`.

## How to test
```bash
cargo test -p ecad-store --features fault-injection
```
Tests are in `tests/disk_contracts.rs`; `dev::power`'s own tests need `fault-injection`. Failure paths are tested
with `MemoryBackend` and `dev::power` (`ecad-file`'s crash tests enable the feature).
