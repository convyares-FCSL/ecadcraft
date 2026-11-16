# ecad-store

Layer: Core

## Purpose
The bytes under the project file: media that records are appended to, and the operating-system
pieces that go with them. It is the one Core crate allowed to touch the OS (disk, lock, clock,
randomness). It does not know what the bytes mean; that is `ecad-file`.

## Contents
- `Medium` (trait): append, flush, truncate and read. `MemoryMedium`: an in-memory medium that can be made to fail, for tests.
- `disk`: `FileMedium` (a file as a medium), `WriterLock` (makes one session the writer), atomic replacement (temporary file, rename, folder flush), and the disk and clock reads that `ecad-file` needs.
- `random_u128`: the system's randomness, for new ids.
- `MemoryBackend`: a redb storage backend in memory, shared by its clones, that can be made to fail once (tests keep a real project file without a disk).
- `power`: a storage backend that simulates power loss, for the container's crash tests.

## Must not depend on
Core rules: no `cadcraft-*`, no UI crate, nothing above Core. In Core it is the only crate where
`std::fs`, `std::env`, `std::process`, clocks and randomness are allowed. It holds no file format
and does not decide the order of write steps.

## Key types
`Medium`, `MemoryMedium`, `FileMedium`, `WriterLock`.

## How to test
```bash
cargo test -p ecad-store
```
Tests are in `tests/disk_contracts.rs`. Failure paths are tested with `MemoryMedium`.
