# ecad-cli

Layer: Programs

## Purpose
The command-line tool `ecad`. It has no commands in this version: `ecad upgrade` went with the
version 1 format (brief 069), and `ecad inspect` says where the developer tool `ecadinspect` is.

## Contents
- `src/main.rs`: the thin `main`.
- `src/lib.rs`: `ecad` (runs a command line), `USAGE`, and the message that says `inspect` moved to `ecadinspect`.

## Must not depend on
Programs rule: no `cadcraft-engine` directly and no Dev crate. It uses only `ecad-model`. It does
not change a project file or look inside one; that is `dev/ecad-inspector`.

## Key types
`ecad_cli::ecad`, `ecad_cli::USAGE`.

## How to test
```bash
cargo test -p ecad-cli
```
Tests are in `tests/all/`. To try the tool by hand, see [`docs/quick-start.md`](../../docs/quick-start.md).
