# ecad-file

Layer: Core

## Purpose
The `.ecad` project file, one format (a redb database): what is written, how records are checked,
and how a file is read back, saved and recovered. Opening the database and its transactions are
`ecad-store`'s (`ecad_store::db`); the tables and what goes in them are here. It reads and
writes through `ecad-store`, so it makes no OS calls of its own.

## Contents
- `records`, `record`: the transaction records the file stores, kept apart from `ecad-model` and converted explicitly, so a model change never changes the format silently.
- `canonical`, `chain`, `crc`: the canonical JSON form (the bytes stored and hashed), the history chain that detects damage and reordering, and CRC-32C.
- `schema`, `store`, `pack`, `chunks`, `project`, `verify`, `disk`: the container (EEP-0005): tables, the database behind the append-only API, compression, snapshots, opening, saving, branches, Verify, and files on disk with the writer lock (Save, Save As). The format version is the number in the file.
- `binding`: a project bound to its file, and the checks on a path typed for one.

## Must not depend on
Core rules: no `cadcraft-*`, no UI. It uses only `ecad-model` and `ecad-store`, the Core crates
before it. It has no OS access except through `ecad-store`. Left out for now: library files beyond
their header (stage 3), checkpoints and other codecs.

## Key types
`Project`, `Store`, `disk::DiskProject`, `binding::Binding`, `ReadOnly`,
`definition_hash`.

## How to test
```bash
cargo test -p ecad-file
```
Tests are in `tests/all/` (crash and recovery tests use in-memory storage); the golden project,
written once by our own build, is in `tests/golden/`. Slow tests are marked `#[ignore = "slow: …"]` and run only at the QA gate.
