# Brief 029: The inspector protocol and `ecadinspect serve`

Status: Draft (designer, 2026-11-12, revised three times the same day after owner decisions;
design `docs/design/029.md`, owner questions §11, dependency note DN1)
Branch: `feature/inspector` (worktree `C:\dev\ecad-features\container-v2`, folder name
historical), from `dev` `dc2472c6`.
Designer: **yes** (`docs/design/029.md`) · Size: M (Verify in `ecad-file` is a store change:
tester and reviewer, per `ecad/CLAUDE.md`)
Roadmap: `docs/roadmap-v0.1.md`, lane C, after 026 and before the look-and-feel pass (owner,
2026-11-11).

## Goal

A developer or tester runs `ecadinspect serve demo.v2.ecad`, opens the local address it prints,
and sees what is in the file: the project and pages, devices, wires and nets, history and
branches, the stored tables, and whether the file checks out. The inspector is a **developer
and test tool** (owner, 2026-11-12). It never ships, and end users never see inside a `.ecad`.

The page talks to the file through a **versioned JSON protocol over a WebSocket**, in the
manner of rosbridge. It is defined in a new developer crate, `dev/ecad-inspector`. Requests carry
ids, so push messages can be added later without breaking it. A later transport can attach the
same protocol to a running app session for live inspection. That transport is not built here.

Developer tools move out of product code into their own tree, `ecad/dev/` (design §12). 029
creates the tree and moves what no other open branch touches. The rest follows in a small brief
once 026, 027 and 028 have merged.

The brief also makes **full Verify correct and fast** in `ecad-file`. Today it checks every
invariant after every transaction and does not finish on the large tier. The new Verify checks
each transaction exactly as commit does, runs the full check at each snapshot and at the end, and
must stay within about 10 s on the large tier. The app's **Verify file**, `ecadinspect verify` and
the check before a revision is issued all use it.

## Owner decisions (2026-11-12)

- No wasm route. No cdylib, exports, `no_mangle`, gzip or size budget. The workspace keeps
  `unsafe_code = "forbid"` untouched.
- The protocol is tested natively. The transport is a **WebSocket** (`tungstenite`, sync,
  pinned), served by `ecadinspect serve [file]` on `127.0.0.1` only, together with the page. The
  page is a thin, read-only client, still opens `ecad-inspect/1` and `/2` snapshots from
  `file://`, and sends no CORS headers.
- The debugger's Verify replay stays. **Developer tools are not interlaced with product code:**
  they live in one tree, product crates do not depend on them (`xtask layers` enforces it), and
  hooks inside a product crate sit in one `dev` module per crate.
- **Verify is fixed here, not worked around** (correction, same day). It has three parts:
  (a) **chain**: decode every row, check the hash chain, the heads and the chunk hashes
  (`verify_chain`, split out, linear); (b) **replay** every branch from the root with only the
  touched-object check per transaction, exactly as at commit; (c) the **full** document and
  index check only at each stored snapshot and at the end of each branch, comparing the state
  with the snapshot. The full check after every transaction (brief 013's verify mode) stays a
  test-only feature and is never used by Verify. No Stop button and no warning.
- Design questions accepted as recommended: no drawing checks or derived locations, v1 files get
  a message and `ecad upgrade`, one file at a time.

## What exists today

- `ecad-inspect` (Core, depends on `ecad-model` only) writes `ecad-inspect/2` JSON snapshots.
  `tools/inspector/index.html` is a static page that reads them.
- `ecad-cli`'s `ecadinspect <file> <view>` prints text views of a v2 file
  (`ecad_file::v2::inspect`, `v2::verify::verify`).
- **Finding (spike, 2026-11-12, design §1):** full Verify on the large bench file (67 MB,
  33 998 transactions) did not finish in 25 minutes, natively in release, because it runs
  brief 013's full check after every transaction (`Ledger::replay_verified_from`). Opening the
  same file takes under a second.

## Scope

1. **Verify** (`ecad_file::v2::verify`, design §3): `verify_chain` and the new `verify`, as above.
   `ecad-model` gains a replay mode that runs the commit-time check only, whatever the build's
   verify mode (design §3.3), so Verify behaves and costs the same in tests and in release. The
   bench gains `verify_chain` and `verify` rows on the large tier.

2. **Protocol** (`ecad_inspector::protocol` in the new `dev/ecad-inspector`, design §2–§4):
   `{"v":1,"id":…,"op":…}` in and `{"v":1,"id":…,"ok":…}` or
   `{"v":1,"id":…,"error":{"what","why","way_out"}}` out. The `event` key and the `subscribe` op
   name are reserved for push. It covers `hello`, the six views, raw rows with a raw/readable
   toggle, the Verify result and `export_jsonl`. Large lists are paged. It is served from a
   **source**: today a file, opened read-only. Designed so that a live session can be a source
   later.
3. **Verify on load** (design §3.4): the page shows the chain result at once and the full
   Verify result when it lands, a few seconds later on the large tier. Verify runs on a
   background thread, and the views stay usable meanwhile.
4. **Transport**: `ecadinspect serve [file] [--port N]`. It serves the page with `GET /` and the
   protocol over a WebSocket at `/ws`, using `tungstenite` (sync, pinned exactly, no TLS:
   dependency note DN1). It is bound to `127.0.0.1` only and accepts one client. Requests need a
   per-run token, and `Host` and `Origin` are checked. A dropped file is sent as a binary frame and
   replaces the open one (design §5).
5. **The page** moves from `tools/inspector/index.html` to `dev/ecad-inspector/page/index.html`
   and gets a **Server** mode (the views as protocol calls) next to the unchanged **Snapshots**
   mode. It stays read-only, and every value is
   escaped.
6. **Mismatches** (design §6): every file or protocol version the tool cannot fully read gets
   a message and a way forward, and the tool shows what it can.
7. **Quick start**: "Inspector snapshots" becomes "The inspector", covering both modes.
8. **Developer tree, part 1** (design §12): create `ecad/dev/`. Move `ecadinspect` out of
   `ecad-cli` (`ecad inspect` gets a pointer) and `ecad_file::v2::inspect` into
   `dev/ecad-inspector`. Remove `tools/`. Move `ecad-model`'s verify-mode code into
   `ecad-model/src/dev.rs`, after #52 merges. `xtask layers` gains the rules of design §12.3,
   with an allowlist that only shrinks. The rest of the tree (`ecad-inspect` split, debugger,
   bench, demo, the `ecad-cadcraft` test features) waits for 026, 027 and 028 to merge, in a
   small brief of its own.
9. The worker also fixes the wasm-only dead-code warning on `Db::ReadOnly`
   (`ecad-file/src/v2/store.rs`) with a `cfg`, since it is in `ecad-file`'s v2 code for
   Verify anyway.

Out of scope: wasm and in-browser reading; attaching to a running app session (only the seam,
design §4); drawing the pages; the drawing Checks rules and derived locations (022, 024–026,
in `ecad-cadcraft`); v1 reading; the debugger's **Verify replay** (`Ledger::replay_verified`,
design Q2); anything that writes.

## Format and ops

None. No FP items. No new command, so no MCP contract test (the 011 rule covers commands).

**DN1, new dependency (for the overseer, owner decision 2026-11-12):** `tungstenite`, sync, pinned
exactly (`=` the current release when the worker starts), `default-features = false` with only
`handshake`. It goes in `dev/ecad-inspector` only and is never in a product crate, which
`xtask layers` enforces. It is MIT OR Apache-2.0. The crates it adds to `Cargo.lock` (expected:
`sha1`, `rand` and its `rand_core`/`rand_chacha`, `data-encoding`, `utf-8`; `http`, `httparse`,
`bytes` and `log` are already there) are listed in the PR, with their licences.

## Acceptance criteria

1. Protocol contract tests (`ecad-inspect/tests/all/protocol_contracts.rs`, table-driven) cover
   every op: the reply's shape, counts and paging, on a fixture built in the test with two
   branches, a revision, a cable and a net.
2. Every row of the mismatch table (design §6) gives its `what` and `way_out`. No input panics,
   including every truncation of a small fixture, a flipped byte in each table, and malformed
   requests and HTTP.
3. **Verify catches what the old one caught**, in a table-driven test that runs the old Verify
   (full check per transaction) and the new one on each case and expects both to fail, naming the
   same transaction or snapshot:
   - chain tamper (a flipped row byte, a missing row, a wrong head);
   - an invariant break at a mid-history transaction (a crafted row with a correct chain);
   - a snapshot that differs from the replay;
   - a missing chunk, and a chunk whose bytes do not hash to its key.
   Each passes on clean fixtures.
4. **A corruption between snapshots is caught at the next snapshot**: a transaction rewritten
   with valid but different ops, its chain and the later chains and head recomputed, passes the
   chain and the replay, and fails at the next snapshot. The report names that snapshot's seq
   and the first difference.
5. **Bench, large tier, native release** (`cargo xtask bench --tier large`): `verify` takes at
   most **10 s** on the large tier's v2 file (`large-lean.v2.ecad`: 33 998 transactions, 500
   pages, 25 000 wires), with a goal of 5 s; `verify_chain` takes at most 3 s. The rows are in the bench report beside open and save.
6. The app's **Verify file**, `ecadinspect verify` and the check before a revision is issued all
   go through the new `verify`, and their existing tests pass unchanged.
7. `ecadinspect serve demo.v2.ecad` prints a `http://127.0.0.1:<port>/?t=<token>` address. Opened,
   it shows all six views in under 1 s. On the large bench file, Project shows in under 3 s.
8. The server refuses `GET /` or a WebSocket upgrade without the token, with a `Host` other than
   `127.0.0.1:<port>` or `localhost:<port>`, or with a foreign `Origin`, and refuses a second
   client (design §5.2). It never listens on another interface. There are tests for each case.
9. On load the chain result shows with the first view, and the Verify result follows without
   blocking the views (**verifying…**, then **verified** or the failure lines).
10. Opening and serving never write the file: the bytes are the same before and after, and the
   file is opened through `Store::open(path, false)`.
11. A fixture whose tag and page title hold `<img src=x onerror=alert(1)>` shows the text and runs
   nothing (the tester checks this in a browser).
12. Snapshots `ecad-inspect/1` and `/2` open in the page as before.
13. `cargo xtask layers` passes with the rules of design §12.3, and fails on a deliberate
    violation of each rule (an `xtask` test). The allowlist holds only the moves listed for later.
14. Quick start "The inspector" is updated, with agent-observed and owner-tried steps kept
    apart.

## Needs owner check

- Answer the remaining questions in design §11.
- A click-only trial: run `ecadinspect serve` on `demo.v2.ecad` and open every view, see
  **verified**, then drop a v1 `.ecad` and a `.lock` file to see the messages.
- The large-tier Verify numbers in the bench report.
