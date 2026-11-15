# ecad-cli

Layer: Programs

## Purpose
The command-line tool `ecad`. It does one thing today: `ecad upgrade <file>` converts a version 1
project file to a version 2 file, checked by replaying both. The source file is never changed.

## Contents
- `src/main.rs`: the thin `main`.
- `src/lib.rs`: `ecad` (runs a command line), `USAGE`, and the message that says `inspect` moved to `ecadinspect`.

## Must not depend on
Programs rule: no `cadcraft-engine` directly and no Dev crate. It uses only `ecad-model` and
`ecad-file`. It does not change a version 2 file or look inside one; that is `dev/ecad-inspector`.

## Key types
`ecad_cli::ecad`, `ecad_cli::USAGE`.

## How to test
```bash
cargo test -p ecad-cli
```
Tests are in `tests/all/`. To try the tool by hand, see [`docs/quick-start.md`](../../docs/quick-start.md).
