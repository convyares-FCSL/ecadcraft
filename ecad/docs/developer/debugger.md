# The debugger and the inspector

Two developer tools for seeing what a project holds: the **debugger**, inside the running app, and the
**inspector**, which looks inside a saved `.ecad` file. Both only read. Neither is for end users.

## The debugger

Open the Text panel (Ctrl+/, or **Commands** in the status bar); its Commands tab holds the ECAD command field
(the exact command names and JSON that MCP uses), **Debugger**, **Inspect Project** and the `ui.primitives`
page. Click **Debugger** to show it, and again to hide it.

It follows one trail: **selected drawing objects → ECAD IDs → model objects → the transactions that touched
them, with the values each change replaced (before) and set (after) → whether each transaction is in the saved
file, and its stored record.**

- **Heading.** `Snapshot: live at seq N` is the transaction shown. The panel copies the project only when
  something changes (an edit, a save, the selection) or on **Refresh**, and draws from that copy. It says how far
  the file is saved and how many transactions are pending (in memory only, lost in a crash). **Stale** means the
  project was busy: the previous copy stays until the next one succeeds.
- **Views.** Selection (what you clicked, and its IDs), Model and Definitions (searchable lists), History (every
  transaction, newest first), Persistence (the file, its lock, what opening found), Storage (the file's head, the
  snapshots, what the last save wrote; table sizes on request), Branches, Diagnostics (pages and runtime events,
  never saved) and Checks.
- **Transaction states.** `[pending]`: only in memory; Save writes it. `[saved]`: durably saved; **Read row**
  reads its stored row and checks that it holds exactly this transaction. `[written, unconfirmed]`: a Save As
  wrote it but could not confirm it is durable; Save confirms it.
- **History at a transaction** rebuilds the document after transaction *n* from the panel's own copy of the
  history. The live project and the drawing do not change.
- **Checks run only on request:** **Verify replay** (replaying the history gives the live document), **Verify
  projection** (the drawing equals a fresh drawing of the document) and **Verify file** (replay from the root
  with every check, compare every snapshot with the replay, compare the rows with the project's history). Each
  shows the transaction and time it ran against.

The headless test `developer::debugger::the_quick_start_debugger_example_runs_headless` in `ecad-app` walks
through placing, wiring, saving, moving, undoing and the checks, and asserts what the panel shows at each step.

### Where the code is

| What | Where |
| --- | --- |
| The data: snapshots, record states, Verify replay, projection and file, reading a row, the state at a transaction | `ecad-engine/src/dev/debug.rs` (with `checks.rs`, `objects.rs`, `verify.rs`) |
| The inspector snapshot an app or agent exports | `ecad-engine/src/dev/snapshot.rs` |
| Test support (`ProjectBuilder`, feature `test-support`) | `ecad-engine/src/dev/test_support.rs` |
| The panel | `ecad-app/src/developer/debugger.rs`; the Commands tab is `ecad-app/src/developer/mod.rs` |

### Read-only, and it must stay so

- It reaches the project only through `ProjectHandle` reads under `try_lock`
  (`ProjectHandle::debug_snapshot`, `debug_projection_inputs`). It copies what it shows and releases the lock
  before anything is drawn. A busy project is never waited for.
- A snapshot is rebuilt only when its key changes (head, saved position, runtime events, active page,
  selection), so an idle panel costs one comparison per frame. Recorded transactions never change, so the panel
  keeps its own copy of the history and each snapshot carries only the new ones.
- Replay, projection and file checks run only when their button is pressed, on copies, outside the lock, on the
  background pool; a result is dropped if the project's head moved meanwhile.
- Nothing in it changes a project. The command field is the one exception, and it runs commands through the
  ordinary command path.

## The inspector

`ecad/dev/ecad-inspector`: a developer and test tool that never ships and that no product crate depends on. It
reads a `.ecad` file and never writes one. Run it from `ecad/`.

**Text views.** On a saved file (the app may have it open):

```sh
cargo run --release -p ecad-inspector --bin ecadinspect -- <file>.ecad tables
```

Views: `tables` (rows and bytes per table, and `meta`), `branches` (each branch, its head and fork point),
`log <branch> --last 20` (the newest transactions, one JSON record each with its stored size), `versions`
(named versions, issued revisions and automatic snapshots), `chunk <hash>` (one snapshot chunk) and `verify` (the
chain, the replay with the commit-time check, then the full check at every snapshot and branch end).

**The page, served.** `ecadinspect serve [<file>.ecad] [--port N]` prints an address on `127.0.0.1` carrying a
per-run token; open it in a browser. The page shows the file's facts, a chain check and Verify's result, and tabs
for the project and pages, devices, wires and nets, history (branches, log, versions), raw tables (each row
**Readable** or **Raw**) and checks. Lists page 200 rows at a time with a search box; times are UTC. Drop another
`.ecad` on the page to look at it (held in memory only); **Reload** re-opens the file named on the command line.
Ctrl+C stops the server.

**Snapshots.** Start the app with `--snapshots <folder>`, then **Export Inspector Snapshot** in the Commands
tab; snapshots are also written before a project is discarded and at exit. They are diagnostic files, not
projects. Open `ecad/dev/ecad-inspector/page/index.html` straight from disk and load them.

How it is built:

- **A versioned JSON protocol** (`protocol`, `PROTOCOL_VERSION`): each request carries the version, an `id`
  echoed in the reply, and an `op`. Every error has a stable `what`, a sentence `why` and a `way_out`. `handle`
  does no I/O, so tests and scripts drive the same views. No op writes.
- **Sources** (`source`): a file opened read-only behind the `Source` trait, its chain checked on load and
  Verify run on a background thread.
- **Safety on a developer machine.** The server listens on `127.0.0.1` only. The page and its WebSocket refuse
  requests without the token, a wrong `Host` or a foreign `Origin`; one page connects at a time; it reads only the
  file named on the command line or bytes the user dropped, never a path from the page.
- **The page** is HTML and JavaScript, allowed only for developer tools under `ecad/dev/`: it ships with no
  product, runs no server by default, never changes a project and escapes every value it shows.
- Tests: `ecad/dev/ecad-inspector/tests/all/` (`protocol_contracts` for every op, every mismatch message,
  hostile files and never writing; `serve_contracts` for the token, `Host`, `Origin`, one page at a time, dropped
  files and limits).

Left out: drawing checks, and attaching to a running app.

## The developer-tool tree

Developer and test tools are never interlaced with product code.

- **They live under `ecad/dev/`**: `ecad-bench`, `ecad-inspector` and `xtask`. A crate is under `dev/` exactly
  when its layer is Dev. Nothing under `dev/` ships.
- **A product crate depends on a `dev/` crate only optionally**, enabled by the product crate's own feature named
  `dev` (written `dev = ["dep:<crate>"]`), and that feature is not on by default. `ecad-app` is the one crate
  allowed to have it on by default, for the desktop app's debugger.
- **Dev features stay in the `dev` module.** In a product crate, the features `dev`, `verify-invariants`,
  `fault-injection` and `test-ops` are read only in `src/dev.rs` or `src/dev/`, or on an attribute line
  directly before a single line that calls into `dev::` or declares `mod dev;`. The engine files whose
  fault-injection and test-ops hooks sit inside the write and access paths they test are listed, with that
  reason.
- **No product source reads a file under `dev/`** (`include_str!`, `include_bytes!`, `include!`, `#[path]`).
- Developer tools may be written in HTML, CSS and JavaScript; anything an end user runs stays Rust.

`cargo xtask layers` enforces all of it ([`guards.md`](guards.md#layers)).
