# Bounded integration experiment: report

> **Written:** 2026-10-12; **revised** after the focused implementation review, and again after
> the owner's manual WSL trial of the desktop harness (same day; see
> [Manual desktop trial](#manual-desktop-trial-on-wsl-user-observed)).
> **Proposal:** `docs/document-architecture-proposal.md` revision 3.1 (§17).
> **Incorporated CADCraft:** `00c7613` (0.6.2). **Product:** `0.1.0-dev`.
> **PRs:** #2 (model, ledger, store), #3 (engine extension interface), #4 (adapter, experiment,
> inspector), #6 (desktop trial harness). The integration choices remain provisional; this report is for review before
> anything expands.

## What exists and what does not

> - **There is no ECAD desktop application.** Everything here runs as Rust tests, one headless
>   example and a minimal desktop **trial harness** (`ecad/apps/ecad-trial`, PR #6). The harness
>   wraps CADCraft's window with the ECAD commands. The owner ran one manual trial of it on WSL.
>   Only the observations listed in [Manual desktop trial](#manual-desktop-trial-on-wsl-user-observed)
>   are verified on a display. CADCraft's own desktop app does not load ECAD commands.
> - **The inspector is a static page that loads exported JSON snapshots.** It has no live
>   connection to a running session and no server, and it does not update by itself.
> - **Persistence (updated 2026-10-13).** When this experiment ran, persistence was test-only and
>   the trial reported that saving was unavailable. The `.ecad` format followed (stage 1, in
>   memory) and then files on disk with Open, Save and Save As in the trial (stage 2a, for
>   review). How to try the current app is in [`quick-start.md`](quick-start.md); the trial
>   below was run before persistence existed.

## Summary

- **The boundary held for every case the experiment exercised.**
  - The ECAD ledger stayed the only history.
  - Each CADCraft commit on an ECAD page became one transaction, or was refused.
  - Undo and redo appended transactions and restored state with identical IDs.
  - Unsaved state, page closing and save failure behaved as specified across two pages.
- **No hypothesis failed outright in the tested scope.** H1 (commit translation) and H5 (owner
  hand-over) needed one adapter-side correction each (see [Findings](#findings)). H3 (connected
  movement) was proved headless only; nobody has looked at the preview on a display yet.
- **Coverage is the main limit.** The scenario uses about a dozen real CADCraft commands. The
  census ran every command but only with default parameters, so most of them stopped at a
  parameter error before touching the drawing.
- **Upstream footprint:**
  - 9 existing upstream files changed: +149 / −8 lines;
  - 1 new engine module, `ext.rs` (353 lines);
  - 2 new engine test files (417 lines);
  - all listed in `ecad/UPSTREAM-PATCHES.md` and guarded by `cargo xtask patches`.

## What ran

All of it ran headless in the cloud container (x86-64 Ubuntu 24.04, Rust 1.97, no display).
`cargo xtask ci` from `ecad/` is green: fmt, clippy `-D warnings`, the ECAD tests, layering, patch
guard, and the wasm build of all ECAD library crates including the adapter. In all, 52 ECAD
tests pass (including two doctests, one of which must fail to compile), plus 14 upstream engine
tests for the interface.

| Test | What it covers |
| --- | --- |
| `ecad-cadcraft/tests/experiment.rs` `scenario_s1_to_s16` | S1-S16 in one project with two pages, checks 1-6 after every step |
| `ecad-cadcraft/tests/refusals.rs` (15 tests) | §17.3: Esc during MOVE and during LINE; EXPLODE, BREAK, HATCH and LAYER.NEW refused; CONNECT to a deleted terminal; injected panics in an ECAD command and in the owner callback; TX-05; interactive group pick; drag preview; closing flow through the real `CadApp`; reopening never reissues IDs; a damaged project file opens its verified prefix read-only with the source preserved |
| `ecad-cadcraft/tests/recovery.rs` (4 tests) | recovery path, checked by content rather than handle numbers: a rebuild after acceptance; owner recovery of two open pages; tampering of each kind is detected; the unavailable-page placeholder (labelled, read only, saveable, close and reopen) |
| `ecad-cadcraft/tests/project_pages.rs` (5 tests) | Save from either page saves the whole project; Save As updates every page's path (including a page opened later); unsupported UNDO options are reported; a `ProjectHandle` refuses a session that is not its own |
| `ecad-cadcraft` `access` unit test and doctests | a held project refuses ECAD commands and handle accessors instead of deadlocking; capturing the session inside `ProjectHandle::read` does not compile |
| `ecad-cadcraft/tests/census.rs` | every CADCraft command's JSON form with default parameters on an ECAD page; checks must hold after each |
| `ecad-model/tests/*` | ledger contracts (11), worked example (§7) |
| `ecad-file/tests/*` (persistence stage 1, replaces the old `ecad-store` test store) | canonical form and CRC framing, record round trip and golden file, recovery classification, save with retry; checkpoints are disabled |
| `ecad-file/tests/disk_contracts.rs`, `ecad-store/tests/disk_contracts.rs` (stage 2a) | files on disk: save, quit and reopen; a failure at every step of Save and Save As; a folder flush left unconfirmed; a crash mid-batch and the tail sidecar; the writer lock, including a killed process; a second opener; external changes; damaged files |
| `ecad-cadcraft/tests/files.rs` (stage 2a) | through the adapter: Save As, Save, close and reopen keep connectivity, IDs and history; a second session is read-only and can save a copy; file names are checked |
| `apps/ecad-trial/tests/trial.rs` (15 tests) | the trial's dialogs (Save As, Open, Save/Discard/Cancel, read-only second window), and the quick-start guide's steps headless |
| `crates/engine/tests/ext_*.rs` (upstream) | the extension interface itself, with a minimal owner: re-entrant calls, a failing or busy close, revisions after delegated undo, unsupported UNDO options |

Checks after every step (§17.4):

1. Each open page's drawing equals a fresh projection, with the same handle map.
2. Replaying the log equals the live document.
3. The handle map is one-to-one; every mapped handle exists and every entity is mapped.
4. The engine's own undo stack is empty for owned pages.
5. Every page tab's unsaved marker equals the head-versus-persisted rule, and so does the engine's
   pointer-based `is_dirty`.
6. No attached conductor end is off its terminal.

## Manual desktop trial on WSL (user-observed)

**Who and when.** The repository owner ran `ecad-trial` (PR #6 at `239d07f`) on 2026-10-12
and reported the results below. They are recorded as the owner observed them. Nothing here was
re-checked by an agent on a display.

**Environment.** Windows laptop, WSL with Ubuntu 26.04, x86_64, WSLg, Rust 1.99.0. The release
build completed. The window opened despite EGL/Zink warnings in the terminal, which were not
investigated further.

| # | Observed by the owner |
| --- | --- |
| 1 | Mouse drawing worked, and so did the banner's Command field. |
| 2 | Undo and redo restored the geometry, while extending the transaction history (as designed). |
| 3 | Snapshot export worked. The inspected snapshot reported valid invariants, replay equality and a matching projection. |
| 4 | Converting the four-line rectangle created a component, and the existing geometry kept its IDs. |
| 5 | Adding a terminal displayed a marker. |
| 6 | PLACE created the trial's rectangular symbol representation. |
| 7 | WIRE created the expected dogleg route. |
| 8 | Moving the component moved its attached wire end. |
| 9 | One undo restored both the component and the wire end together. |
| 10 | A second page opened and kept its own separate drawing content. |
| 11 | Closing page 2's tab showed no discard prompt. |
| 12 | Save reported that persistence was unavailable. |
| 13 | Closing the last page warned that the design would be lost, and Cancel kept it. |

**Not covered by this trial.** These remain unverified on a display, whatever the headless tests
show:
- moving or rotating a placed (rotated) symbol with wires attached;
- reopening a closed page (`openpage`);
- the drag preview, beyond the result after the move: the wire end following during the drag, and
  every other preview detail;
- **Discard**, quitting with an edited project, Close All, and the snapshot written on discard or
  at exit;
- Save As through the menu;
- every other command not listed above, including COPY, COPYCLIP/PASTECLIP, ERASE, CONNECT,
  DISCONNECT, ECADDELETE, SETPROP, CABLEADD and the refused commands;
- recovery paths (rebuild, placeholder page), which only fault injection in tests reaches.

**What it supports.** For the operations observed, the trial agrees with the headless results on
several points:
- connected movement with H3 at commit;
- one undoable transaction per command, with the ECAD ledger owning history (H5);
- the save and close behaviour required of the harness;
- the inspector reading a real session's snapshot.

It adds no evidence for H1 beyond the commands listed, and none for H4 (copies).

## Results by hypothesis

| # | Hypothesis | Result | Evidence |
| --- | --- | --- | --- |
| H1 | CADCraft commits translate into correct transactions by diffing against the last projection plus provenance | **Held for the tested commands** | LINE (interactive and JSON), TEXT, MOVE (JSON and interactive group pick), ROTATE, GRIP.STRETCH, COPY, COPYCLIP/PASTECLIP, CUTCLIP, ERASE, REVERSE. Refused cleanly: EXPLODE, BREAK, JOIN, ARRAY, ARRAYRECT, GROUP, LAYCUR, LAYISO/LAYFRZ/LAYOFF/LAYLCK, LAYER.NEW, LAYOUT.NEW, CHSPACE, AUTOCONSTRAIN, HATCH (census + refusals) |
| H2 | The projection round trip is lossless for supported kinds, including symbol placement | **Held** | normal path: zero `FidelityMismatch` events across the scenario (asserted) and check 1 after every step. Recovery path: every rebuilt page passes `verify_page`, the content check that does not use handle numbers (finding 10). A placed symbol rotated 90° and then 180° reproduces its terminal positions |
| H3 | Moving connected equipment feels normal: ends follow at commit, the preview shows them | **Held at commit (also seen by the owner on WSL for a drawn component); preview shown only by test** | S6 (MOVE of a drawn component, ROTATE of a symbol) and the interactive group-pick test; `drag_preview_shows_following_wire_ends` checks `Session::drag` returns the following route. Not seen on screen |
| H4 | A plain copy of a whole component is an independent copy with preserved internal relationships | **Held within one page** | S8: COPY of a component with an attached wire gives a new component, 3 new terminals, a new wire attached to the copied terminal, and unchanged originals; COPYCLIP/PASTECLIP of a symbol gives a new component with terminals A1/A2 on the same embedded definition |
| H5 | Owner-delegated undo, save, unsaved state and close keep one history and correct prompts | **Held after one adapter correction** | S10-S14; `closing_flow_prompts_for_the_project_not_for_page_tabs` through the real `CadApp`: a page tab closes without a prompt, closing the project prompts, and a failed save keeps it open |
| H6 | The interface survives upstream merges | **Not measurable yet** | needs successive upstream syncs |

## Findings

1. **UI Save on a document without a path opens Save As** (`ui-egui/src/menus.rs:156`), so it
   never reaches the owner. Fixed in the adapter, not upstream: page documents carry the
   project's file path. A never-saved project therefore gets Save As, which reaches the owner with
   the chosen path. No extra upstream patch was needed.
2. **Recovery renumbers handles** (accepted). After an owner panic, or a failed projection after
   acceptance (PROJ-02), pages are rebuilt from the document with fresh handles. The document,
   IDs and history are untouched; the engine-side selection is lost, which is acceptable. Each
   rebuilt page is now verified by content (finding 10).
3. **An unavailable page becomes a labelled placeholder, not a closed tab** (accepted). The engine
   cannot safely remove a document from inside a commit callback, and no engine hook is added
   for it. The page shows one text, "PAGE UNAVAILABLE…", and its tab title ends in
   "[unavailable]". It refuses every edit: CADCraft commands, ECAD commands and undo. Edits on
   other pages do not revive or change it. The project still saves from any page, and closing and
   reopening the page reprojects it (S16, `an_unavailable_page_is_a_labelled_read_only_placeholder`).
4. **Locking is now enforced by the API, not by caller discipline.**
   - **The hazard:** while the project is locked, an owner-aware `Session` call (`execute`,
     `undo`, `doc_unsaved`, save, close…) cannot succeed. Before this review the test harness
     deadlocked twice and once got a wrong answer that way.
   - **The boundary** (`ecad-cadcraft/src/access.rs`):
     - Outside the crate a project is reachable only through `ProjectHandle`. The `Arc<Mutex>`
       is private.
     - Its accessors (`read`, `configure`, `snapshot`, `open_page`) lock with `try_lock`, so a
       busy project is an error, not a deadlock.
     - They take `&mut Session` for the call, so a closure that captures the session does not
       compile. A doctest proves this (`compile_fail`).
     - They refuse a session that does not hold the project's pages. Passing an unrelated
       session to fake the proof therefore fails.
     - Inside the crate, project code never receives a `Session`. It gets `Pages`, which exposes
       only document lookup and replacement, opening and binding a page, and echo. It has none of
       the owner-aware methods.
     - ECAD commands lock with `try_lock` too.
   - **The engine side** (PR #3) uses `try_lock` as well. A re-entrant command is refused and its
     drawing restored. State queries answer conservatively ("unsaved"). A failing or busy close
     leaves the document open.
   - **Tests:**
     - the `access` unit test: a held project refuses commands and accessors;
     - `a_reentrant_owner_call_is_refused_not_deadlocked` (engine);
     - `a_project_handle_only_works_with_its_own_session`;
     - the `compile_fail` doctest.
   - The move to this API also caught a real misuse in the tests: the closing-flow test changed
     a project after its session had been moved into the `CadApp`.
5. **Unsaved state reaches the engine two ways.** The upstream patch makes tabs, close and quit
   ask the owner (`doc_unsaved`, `close_needs_prompt`, `has_unsaved_work`). The adapter also keeps
   each page's `DocState::saved` pointer in step, so code still calling `is_dirty` directly
   (`DBMOD`, `summary`, `inquiry`) agrees. That second mechanism is indirect. It works and is
   tested (check 5), but upstreaming the owner-aware query everywhere would be cleaner.
6. **Replacement edits are refused generically.** One rule, independent of command names, covers
   BREAK, JOIN and EXPLODE (and any future command that re-creates objects): "an object with a
   meaning was removed and new objects without provenance appeared in the same commit".
7. **REVERSE swaps a wire's ends** and was recorded correctly: M3 re-resolved both ends to the
   terminals they now touch, so the attachments swapped and connectivity stayed the same.
8. **Placement rotation is stored in radians**, matching CADCraft's insert so the round trip is
   exact. The proposal (§4.1) says degrees; the PLACE command still takes degrees.
9. **Upstream test `cadcraft-cli` `mcp_connect::connect_to_closed_port_fails`: unresolved,
   suspected flaky.** It failed in one full CADCraft run and passed in the latest one. It does not
   touch the patched code. Port reuse under parallel tests is a plausible explanation, but it is
   not established. A passing run is not treated as proof.
10. **Two projection checks, for two paths.**
    - **Normal path, the fidelity check.** After a CADCraft commit, the page is reprojected with
      the engine's handles kept, and each entity is compared by handle with what the engine
      committed (`FidelityMismatch`).
    - **Why it cannot cover recovery.** After a rebuild every handle is new. The first version of
      the check compared old and new handles and reported every object as different (37 in
      S16), so it now runs on the normal path only.
    - **Recovery path, `verify_page`** (`ecad-cadcraft/src/verify.rs`). It decodes the rebuilt
      drawing back into ECAD terms and compares it with the authoritative document by content:
      - every item's geometry and style, as a multiset and through the map;
      - each symbol's insert and placement;
      - drawn terminal markers;
      - symbol terminal positions, read from the block's markers and the insert's transform;
      - symbol blocks against their definitions;
      - group membership;
      - the map's one-to-one coverage;
      - attached conductor ends lying on terminals read from the drawing.
    - **Outcome.** A pass is recorded as `RecoveryVerified`. A failure is recorded as
      `RecoveryVerificationFailed`, and the page becomes the unavailable placeholder.
    - **Independent test comparison.** The tests add their own check. They compare content
      keyed by stable IDs with a projection whose handles share no number with the live one,
      check that the document is unchanged by recovery, and tamper with each aspect in turn to
      show that it is detected (`tests/recovery.rs`).

## Census (default parameters)

399 commands ran (those that open, close or create documents, quit, or wait were skipped):

| Outcome | Count | Notes |
| --- | ---: | --- |
| recorded as a transaction | 6 | `u`, `undo`, `cutclip`, `erase.selection`, `erase`, `reverse` |
| refused by the owner | 14 | EXPLODE, JOIN (replacement); ARRAY, ARRAYRECT (block definitions); CHSPACE, LAYOUT.NEW (layouts); GROUP, -GROUP (groups); LAYCUR (would move a terminal marker to another layer); LAYISO/LAYFRZ/LAYOFF/LAYLCK (layers); AUTOCONSTRAIN (constraints) |
| no recorded change | 143 | view, settings, queries, and commands that changed only session state |
| command error before any change | 236 | most need parameters |

After every command, checks 1-6 held. Coverage is shallow: a census with real parameters for
each command is needed before any limit is lifted.

## Remaining limitations

| # | Limitation (experiment only) | Status |
| --- | --- | --- |
| L1 | EXPLODE, BREAK, JOIN and other replacements refused on linked items | in force (generic rule, finding 6) |
| L2 | Copying part of a representation gives plain items | implemented; not yet in a test |
| L3 | Cables are not copied; copied members lose membership | in force |
| L4 | Copy and paste within one page; cross-page paste refused | in force |
| L5 | ERASE removes representations; DELETE deletes subjects | in force (S9) |
| L6 | Hatch, dimensions, tables, MText, arbitrary inserts, MINSERT, non-uniform scale refused | in force |
| L7 | No autosave or crash journal; UNDO Mark/Back/BEgin/End/Auto/Control not delegated | in force; on ECAD pages they are **reported as unsupported** (JSON and command line); UNDO by count works |
| L8 | Placement by command only; no drag from a panel | in force |
| L9 | Test-only store (JSON lines in memory); Save records a path but writes no file | in force |
| L10 | ECAD edits refused while a CADCraft command runs (TX-05) | in force |
| new | Layer, layout, group, constraint and block-definition edits refused on ECAD pages | from the census |
| new | ECAD commands are JSON-only (no interactive prompts) | experiment scope |
| new | Provenance covers COPY-family transforms and PASTECLIP only (not grip copy, ARRAY, PASTEBLOCK) | copies through the others are refused |
| new | Closing a project's last page closes the project | experiment choice; product behaviour still deferred |
| new | Drafting-command restrictions (L1, L6, census refusals) | kept until parameterised tests pass |
| new | No ECAD desktop shell; inspector reads exported snapshots only | see [What exists](#what-exists-and-what-does-not) |

## Upstream patch footprint (actual)

| File | Change |
| --- | --- |
| `crates/engine/src/ext.rs` | new, 353 lines |
| `crates/engine/src/lib.rs` | +63 / −1 |
| `crates/engine/src/cmd/file.rs` | +21 |
| `crates/engine/src/cmd/mod.rs` | +20 |
| `crates/engine/src/cmd/edit.rs` | +18 (UNDO options reported as unsupported on owned documents) |
| `crates/engine/src/cmd/clipboard.rs` | +11 / −2 |
| `crates/engine/src/cmd/modify.rs` | +6 |
| `crates/ui-egui/src/closing.rs` | +6 / −3 |
| `crates/engine/src/autosave.rs` | +2 / −1 |
| `crates/ui-egui/src/chrome.rs` | +2 / −1 |
| `crates/engine/tests/ext_owner.rs`, `ext_commands.rs` | new, 417 lines |

The full CADCraft suite (`cargo test --workspace` in the repository root) was run with these
patches at PR #3 head `fd2c814`; PR #4 changes nothing outside `ecad/`. The latest run gave
**1,674 passed and 0 failed**. An earlier full run on this branch gave one failure, the
unresolved test in finding 9. Clippy, fmt, layering and wasm were clean.

## Developer inspector

> The static inspector stays for exported diagnostics. A live, read-only debugger panel in the
> trial application is proposed (not built) in architecture proposal Part E, §21.

- **JSON interface:** `ecad-inspect` builds snapshots from the model's own serialisation
  (`Document`, `Transaction`), plus a label index and check results. Each snapshot is labelled
  with its transaction sequence. A historical snapshot reconstructs that state with
  `Ledger::state_at` and never touches the live project; tested in
  `snapshots_are_labelled_and_history_is_separate`.
- **Runtime events** (accepted, refused, saved, save failed, projection failed or rebuilt,
  recovery verified or failed, page unavailable, recovered, clipboard captured) live in a bounded event log that is never saved
  or replayed. Each event carries its command id and transaction sequence.
- **Getting snapshots:**
  - the read-only `ecadinspect {seq?}` command, in a session with the ECAD commands installed
    (`engine.execute` or scripts in tests and the example). No shipped app installs them yet, so
    the CADCraft desktop app's control channel cannot reach it;
  - `cargo run -p ecad-cadcraft --example experiment_snapshots -- <dir>`, which writes one live
    snapshot and three historical ones.
- **Viewer:** `ecad/tools/inspector/index.html` is a static page with no external scripts and no
  server. It opens snapshot files and shows:
  - the project, its pages, and head and persisted positions;
  - components, terminals, conductors and cables, with their relationships;
  - searchable IDs and labels;
  - transactions with their ops and before/after values;
  - runtime events, with failures highlighted;
  - live-versus-replay and projection checks.
  Checked in headless Chromium: no console errors, historical and live snapshots both load,
  search works, no horizontal scroll at 390 px.
- **Snapshots only; no live view.** The page shows files someone exported and nothing else; it
  never talks to a running session. Live updates were not added. Any later live server must
  bind to `127.0.0.1` only and expose no mutation.

## Environments

See [`environments.md`](environments.md):

- the checks to run on WSL (first local route) and on the Thor (later ARM64 target, where binaries
  must be built natively or cross-compiled);
- build requirements (Rust ≥ 1.90, a C toolchain, 6-8 GB disk with debug info off);
- how to run the headless experiment and the inspector.

Neither environment has been tried yet.

## Decisions

Recorded from the review (2026-10-12):

- The generic extension interface (PR #3) is **retained provisionally**. It is **not** offered
  upstream yet.
- Page documents carry the project path as adapter metadata (finding 1). Save from either page
  saves the whole project, and Save As updates every page's path (tested).
- Losing the selection after recovery is acceptable; stable IDs and content must survive
  (finding 2, tested).
- Unavailable pages stay labelled, non-editable placeholders. There is no engine hook to close
  them automatically, and the close-and-reopen route is kept (finding 3).
- Locking is resolved through an enforceable API boundary (finding 4).
- Placement rotation is stored in radians, and commands and UI convert at the boundary (proposal
  revision 3.1, finding 8).
- The drafting-command restrictions stay until meaningful parameterised tests pass. That
  coverage is not expanded yet.

Still open for review:

1. **Next coverage step:** a parameterised census of the drawing commands most likely to be used
   on schematics (TRIM, EXTEND, FILLET, STRETCH, MIRROR, grip copy, ARRAY), before any limitation
   is lifted.
2. **Product behaviour still deferred:** what ERASE and Delete do by default, the copy meanings
   "another contact" and "another depiction", tag on copy, whether closing the last page closes
   the project, and orthogonal tidy-up when wire ends follow.
3. **The smallest runnable ECAD desktop harness** for a manual WSL trial (proposed separately,
   not built).
