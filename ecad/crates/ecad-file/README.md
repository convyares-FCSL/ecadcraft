# ecad-file

Layer: Core

## Purpose
The `.ecad` project file: what is written, how records are framed and checked, and how a file is
read back, saved and recovered. It reads and writes through `ecad-store`, so it makes no OS calls
of its own.

## Contents
- `record`, `v1`, `v2`: the file's own records, kept apart from `ecad-model` and converted explicitly, so a model change never changes the format silently. `v2` is the version 2 container.
- `codec`, `crc`, `canonical`, `chain`: one framed record per line with a CRC, the canonical JSON form, and the history chain that detects damage and reordering.
- `read`: opens a file and reports what follows the verified part (clean, an unfinished last write, or corruption).
- `write`: first write and Save with retries over a `Medium`.
- `disk`: opening a file on disk with the writer lock, Save, Save As.

## Must not depend on
Core rules: no `cadcraft-*`, no UI. It uses only `ecad-model` and `ecad-store`, the Core crates
before it. It has no OS access except through `ecad-store`. Left out for now: library files beyond
their header (stage 3), checkpoints and other codecs.

## Key types
`open_project`, `ProjectFile`, `DiskProject`, `OpenedDisk`, `Header`, `OpenError`, `SaveError`,
`definition_hash`.

## How to test
```bash
cargo test -p ecad-file
```
Tests are in `tests/all/` (crash and recovery tests use in-memory media); sample files are in
`tests/golden/`. Slow tests are marked `#[ignore = "slow: …"]` and run only at the QA gate.
