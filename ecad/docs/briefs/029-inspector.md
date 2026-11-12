# Brief 029: The inspector protocol and `ecadinspect serve`

Status: Draft (designer, 2026-11-12, revised the same day after owner decisions; design
`docs/design/029.md`, owner questions §11)
Branch: `feature/inspector` (worktree `C:\dev\ecad-features\container-v2`, folder name
historical), from `dev` `dc2472c6`.
Designer: **yes** (`docs/design/029.md`) · Size: S
Roadmap: `docs/roadmap-v0.1.md`, lane C, after 026 and before the look-and-feel pass (owner,
2026-11-11).

## Goal

A developer or tester runs `ecadinspect serve demo.v2.ecad`, opens the local address it prints,
and sees what is in the file: the project and pages, devices, wires and nets, history and
branches, the stored tables, and whether the file checks out. The inspector is a **developer
and test tool** (owner, 2026-11-12). It never ships, and end users never see inside a `.ecad`.

The page talks to the file through a **versioned JSON request/response protocol** defined in
`ecad-inspect`, in the manner of rosbridge. A later transport can attach the same protocol to a
running app session for live inspection. That transport is not built here.

## Owner decisions (2026-11-12)

- No wasm route. No cdylib, exports, `no_mangle`, gzip or size budget. The workspace keeps
  `unsafe_code = "forbid"` untouched.
- The protocol lives in `ecad-inspect` and is tested natively. The transport is
  `ecadinspect serve [file]` on `127.0.0.1` only. `tools/inspector/index.html` becomes a thin,
  read-only client and still opens `ecad-inspect/1` and `/2` snapshots.
- **Check chain** (split out of Verify) runs on load. Full Verify runs on request, with
  **Stop**. Making full Verify fast is a separate `ecad-file` brief and is not solved here.
- Design questions accepted as recommended: no drawing checks or derived locations, v1 files get
  a message and `ecad upgrade`, one file at a time.

## What exists today

- `ecad-inspect` (Core, depends on `ecad-model` only) writes `ecad-inspect/2` JSON snapshots.
  `tools/inspector/index.html` is a static page that reads them.
- `ecad-cli`'s `ecadinspect <file> <view>` prints text views of a v2 file
  (`ecad_file::v2::inspect`, `v2::verify::verify`).
- **Finding (spike, 2026-11-12, design §1):** full Verify on the large bench file (67 MB,
  33 998 transactions) did not finish in 25 minutes, natively in release. Opening it takes
  under a second.

## Scope

1. **Protocol** (`ecad_inspect::protocol`, design §2–§4): `{"v":1,"id":…,"op":…}` in and
   `{"v":1,"id":…,"ok":…}` or `{"v":1,"id":…,"error":{"what","why","way_out"}}` out. It covers
   `hello`, the six views, raw rows with a raw/readable toggle, Check chain, Verify (start,
   status, stop) and `export_jsonl`. Large lists are paged. It is served from a **source**:
   today a file, opened read-only. Designed so that a live session can be a source later.
   `ecad-inspect` gains `ecad-file` as a dependency. It stays Core and still builds for wasm.
2. **`verify_chain`** in `ecad_file::v2::verify`: verify's first phase on its own (decode every
   row, check the hash chain and heads, check chunk hashes), with no replay. Code, not format.
3. **Transport**: `ecadinspect serve [file] [--port N]` in `ecad-cli`. It speaks plain HTTP/1.1
   on std `TcpListener`, bound to `127.0.0.1` only, with a per-run token. It serves the page,
   `POST /api` and `POST /open` (a dropped file's bytes, held in memory, replacing the one open
   before). **No new crate:** `ecad-cli` takes `ecad-inspect`, `serde_json` and `sha2` (all already in
   the lock) as direct dependencies (design §5).
4. **The page**: `tools/inspector/index.html` gets a **Server** mode (the views as protocol
   calls) next to the unchanged **Snapshots** mode. It stays read-only, and every value is
   escaped.
5. **Mismatches** (design §6): every file or protocol version the tool cannot fully read gets
   a message and a way forward, and the tool shows what it can.
6. **Quick start**: "Inspector snapshots" becomes "The inspector", covering both modes.
7. The worker also fixes the wasm-only dead-code warning on `Db::ReadOnly`
   (`ecad-file/src/v2/store.rs`) with a `cfg`, since it touches `ecad-file`'s v2 code for
   `verify_chain` anyway.

Out of scope: wasm and in-browser reading; attaching to a running app session (only the seam,
design §4); drawing the pages; the drawing Checks rules and derived locations (022, 024–026,
in `ecad-cadcraft`); v1 reading; making full Verify fast (its own `ecad-file` brief, design §7);
anything that writes.

## Format and ops

None. No FP items. No new command, so no MCP contract test (the 011 rule covers commands).

## Acceptance criteria

1. Protocol contract tests (`ecad-inspect/tests/all/protocol_contracts.rs`, table-driven) cover
   every op: the reply's shape, counts and paging, on a fixture built in the test with two
   branches, a revision, a cable and a net.
2. Every row of the mismatch table (design §6) gives its `what` and `way_out`. No input panics,
   including every truncation of a small fixture, a flipped byte in each table, and malformed
   requests and HTTP.
3. `verify_chain` passes on the demo and on every fixture. It finds a flipped row byte, a missing
   row and a wrong head. On the large bench file it takes under 5 s in release.
4. `ecadinspect serve demo.v2.ecad` prints a `http://127.0.0.1:<port>/?t=<token>` address. Opened,
   it shows all six views in under 1 s. On the large bench file, Project shows in under 3 s.
5. The server refuses any request without the token, with a `Host` other than `127.0.0.1:<port>`
   or `localhost:<port>`, or with a foreign `Origin` (design §5.2). It never listens on another
   interface. There are tests for each case.
6. Full Verify runs on request in a child process (design §3.3). It shows the elapsed time and
   how many transactions it is replaying, and **Stop** ends it within 1 s. The views stay usable
   meanwhile.
7. Opening and serving never write the file: the bytes are the same before and after, and the
   file is opened through `Store::open(path, false)`.
8. A fixture whose tag and page title hold `<img src=x onerror=alert(1)>` shows the text and runs
   nothing (the tester checks this in a browser).
9. Snapshots `ecad-inspect/1` and `/2` open in the page as before.
10. Quick start "The inspector" is updated, with agent-observed and owner-tried steps kept
    apart.

## Needs owner check

- Answer the remaining questions in design §11.
- A click-only trial: run `ecadinspect serve` on `demo.v2.ecad`, open every view, run Verify and
  stop it, and drop a v1 `.ecad` and a `.lock` file to see the messages.
