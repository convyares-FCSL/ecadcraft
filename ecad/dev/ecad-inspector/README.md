# ecad-inspector

Layer: Dev

## Purpose
A developer tool for looking inside a `.ecad` file without changing it: text views on the command
line, and a web page served on this machine. It never ships, and no product crate depends on it.

## Contents
- `protocol`: a versioned JSON protocol (`PROTOCOL_VERSION`), handled by `handle`, which does no I/O.
- `source`: a file opened read-only (`FileSource`, behind the `Source` trait), chain check on load, Verify on a background thread.
- `serve`: `ecadinspect serve`, the page and the protocol over a WebSocket on 127.0.0.1.
- `text`, `cli`: `ecadinspect <file> <view>`, the text views.
- `page/index.html`: the inspector page (HTML and JavaScript, allowed for developer tools; it escapes every value it shows).
- `src/bin/ecadinspect.rs`: the program.

## Must not depend on
Dev may use any crate; this one uses `ecad-file`, `ecad-model` and a WebSocket crate. Nothing here
writes a project file. Left out: drawing checks and attaching to a running app.

## Key types
`source::Source`, `source::FileSource`, `protocol::handle`.

## How to test
```bash
cargo test -p ecad-inspector
```
Tests are in `tests/all/`. To run the tool, see [`docs/quick-start.md`](../../docs/quick-start.md).
