# Bounded integration experiment: report

> **Written:** 2026-10-12. **Proposal:** `docs/document-architecture-proposal.md` revision 3 (§17).
> **Incorporated CADCraft:** `00c7613` (0.6.2). **Product:** `0.1.0-dev`.
> **PRs:** #2 (model, ledger, store), #3 (engine extension interface), #4 (adapter, experiment,
> inspector). The integration choices remain provisional; this report is for review before
> anything expands.

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
  - 8 existing upstream files changed: +126 / −8 lines;
  - 1 new engine module, `ext.rs` (308 lines);
  - 2 new engine test files (299 lines);
  - all listed in `ecad/UPSTREAM-PATCHES.md` and guarded by `cargo xtask patches`.

## What ran

All of it ran headless in the cloud container (x86-64 Ubuntu 24.04, Rust 1.97, no display).
`cargo xtask ci` from `ecad/` is green: fmt, clippy `-D warnings`, the ECAD tests, layering, patch
guard, and the wasm build of all ECAD library crates including the adapter. In all, 32 tests
pass, plus 9 upstream engine tests for the interface.

| Test | What it covers |
| --- | --- |
| `ecad-cadcraft/tests/experiment.rs` `scenario_s1_to_s16` | S1-S16 in one project with two pages, checks 1-6 after every step |
| `ecad-cadcraft/tests/refusals.rs` (9 tests) | §17.3: Esc during MOVE and during LINE; EXPLODE, BREAK, HATCH and LAYER.NEW refused; CONNECT to a deleted terminal; injected panics in an ECAD command and in the owner callback; TX-05; interactive group pick; drag preview; closing flow through the real `CadApp` |
| `ecad-cadcraft/tests/census.rs` | every CADCraft command's JSON form with default parameters on an ECAD page; checks must hold after each |
| `ecad-model/tests/*` | ledger contracts (11), worked example (§7) |
| `ecad-store/tests/*` | save, torn line, failed save after a new edit, disposable checkpoints |
| `crates/engine/tests/ext_*.rs` (upstream) | the extension interface itself, with a minimal owner |

Checks after every step (§17.4):

1. Each open page's drawing equals a fresh projection, with the same handle map.
2. Replaying the log equals the live document.
3. The handle map is one-to-one; every mapped handle exists and every entity is mapped.
4. The engine's own undo stack is empty for owned pages.
5. Every page tab's unsaved marker equals the head-versus-persisted rule, and so does the engine's
   pointer-based `is_dirty`.
6. No attached conductor end is off its terminal.

## Results by hypothesis

| # | Hypothesis | Result | Evidence |
| --- | --- | --- | --- |
| H1 | CADCraft commits translate into correct transactions by diffing against the last projection plus provenance | **Held for the tested commands** | LINE (interactive and JSON), TEXT, MOVE (JSON and interactive group pick), ROTATE, GRIP.STRETCH, COPY, COPYCLIP/PASTECLIP, CUTCLIP, ERASE, REVERSE. Refused cleanly: EXPLODE, BREAK, JOIN, ARRAY, ARRAYRECT, GROUP, LAYCUR, LAYISO/LAYFRZ/LAYOFF/LAYLCK, LAYER.NEW, LAYOUT.NEW, CHSPACE, AUTOCONSTRAIN, HATCH (census + refusals) |
| H2 | The projection round trip is lossless for supported kinds, including symbol placement | **Held** | zero `FidelityMismatch` events across the scenario (asserted, after correcting the check in finding 10); check 1 after every step; a placed symbol rotated 90° and then 180° reproduces its terminal positions |
| H3 | Moving connected equipment feels normal: ends follow at commit, the preview shows them | **Held at commit; preview shown only by test** | S6 (MOVE of a drawn component, ROTATE of a symbol) and the interactive group-pick test; `drag_preview_shows_following_wire_ends` checks `Session::drag` returns the following route. Not seen on screen |
| H4 | A plain copy of a whole component is an independent copy with preserved internal relationships | **Held within one page** | S8: COPY of a component with an attached wire gives a new component, 3 new terminals, a new wire attached to the copied terminal, and unchanged originals; COPYCLIP/PASTECLIP of a symbol gives a new component with terminals A1/A2 on the same embedded definition |
| H5 | Owner-delegated undo, save, unsaved state and close keep one history and correct prompts | **Held after one adapter correction** | S10-S14; `closing_flow_prompts_for_the_project_not_for_page_tabs` through the real `CadApp`: a page tab closes without a prompt, closing the project prompts, and a failed save keeps it open |
| H6 | The interface survives upstream merges | **Not measurable yet** | needs successive upstream syncs |

## Findings

1. **UI Save on a document without a path opens Save As** (`ui-egui/src/menus.rs:156`), so it
   never reaches the owner. Fixed in the adapter, not upstream: page documents carry the
   project's file path. A never-saved project therefore gets Save As, which reaches the owner with
   the chosen path. No extra upstream patch was needed.
2. **Recovery renumbers handles.** After an owner panic, or a failed projection after acceptance
   (PROJ-02), pages are rebuilt from the document with fresh handles. The document, IDs and
   history are untouched, but the engine-side selection is lost. This is acceptable for a
   recovery path. Reusing the old map is possible but trusts state that just failed.
3. **An unavailable page becomes an empty placeholder, not a closed tab.** The engine cannot
   safely remove a document from inside a commit callback. The page shows nothing, refuses edits
   and says why; the project stays saveable; closing and reopening the page reprojects it (S16).
   This departs from the proposal's wording ("the page view closes").
4. **Locking discipline is a real hazard.** Owners are `Arc<Mutex<…>>`. The engine side uses
   `try_lock` and refuses a re-entrant call instead of deadlocking, so a busy owner answers
   "unsaved" to `doc_unsaved`, the safe answer. The test harness deadlocked twice by holding the
   project lock across a second lock, and once got a wrong answer by holding it while asking the
   session. ECAD code must never hold the project lock while calling owner-aware `Session`
   methods. A future refinement could pass the owner by `&mut` through the callbacks only.
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
9. **Unrelated flaky upstream test**: `cadcraft-cli` `mcp_connect::connect_to_closed_port_fails`
   failed once in the full CADCraft run (ephemeral port reuse under parallel tests) and passed 3
   out of 3 times on its own.
10. **The fidelity check itself was wrong after a rebuild.** When PROJ-02 rebuilds a page with
    fresh handles, comparing the engine's committed drawing (old handles) with the rebuild
    reported every object as different (37 in S16). The check now runs only when the normal
    projection kept the engine's handles. The scenario asserts zero mismatches.

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
| L7 | No autosave or crash journal; UNDO Mark/Back/BEgin/End not delegated | in force; UNDO options operate on the engine's empty stack and do nothing |
| L8 | Placement by command only; no drag from a panel | in force |
| L9 | Test-only store (JSON lines in memory) | in force |
| L10 | ECAD edits refused while a CADCraft command runs (TX-05) | in force |
| new | Layer, layout, group, constraint and block-definition edits refused on ECAD pages | from the census |
| new | ECAD commands are JSON-only (no interactive prompts) | experiment scope |
| new | Provenance covers COPY-family transforms and PASTECLIP only (not grip copy, ARRAY, PASTEBLOCK) | copies through the others are refused |
| new | Closing a project's last page closes the project | experiment choice; product behaviour still deferred |

## Upstream patch footprint (actual)

| File | Change |
| --- | --- |
| `crates/engine/src/ext.rs` | new, 308 lines |
| `crates/engine/src/lib.rs` | +63 / −1 |
| `crates/engine/src/cmd/mod.rs` | +20 |
| `crates/engine/src/cmd/file.rs` | +16 |
| `crates/engine/src/cmd/clipboard.rs` | +11 / −2 |
| `crates/engine/src/cmd/modify.rs` | +6 |
| `crates/ui-egui/src/closing.rs` | +6 / −3 |
| `crates/engine/src/autosave.rs` | +2 / −1 |
| `crates/ui-egui/src/chrome.rs` | +2 / −1 |
| `crates/engine/tests/ext_owner.rs`, `ext_commands.rs` | new, 299 lines |

The full CADCraft suite with these patches gave 1,668 passed and 1 failed. The failure is the
flaky test in finding 9. Clippy, fmt, layering and wasm were clean.

## Developer inspector

- **JSON interface:** `ecad-inspect` builds snapshots from the model's own serialisation
  (`Document`, `Transaction`), plus a label index and check results. Each snapshot is labelled
  with its transaction sequence. A historical snapshot reconstructs that state with
  `Ledger::state_at` and never touches the live project; tested in
  `snapshots_are_labelled_and_history_is_separate`.
- **Runtime events** (accepted, refused, saved, save failed, projection failed or rebuilt,
  page unavailable, recovered, clipboard captured) live in a bounded event log that is never saved
  or replayed. Each event carries its command id and transaction sequence.
- **Getting snapshots:**
  - the read-only `ecadinspect {seq?}` command, usable through the control channel, `engine.execute`
    or scripts;
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
- **Live updates were not added.** Any later live server must bind to `127.0.0.1` only and
  expose no mutation.

## Environments

See [`environments.md`](environments.md):

- the checks to run on WSL (first local route) and on the Thor (later ARM64 target, where binaries
  must be built natively or cross-compiled);
- build requirements (Rust ≥ 1.90, a C toolchain, 6-8 GB disk with debug info off);
- how to run the headless experiment and the inspector.

Neither environment has been tried yet.

## Decisions for review (nothing expanded)

1. **Accept or adjust the generic extension interface** (PR #3) as the upstream patch set, and
   decide whether to offer it upstream.
2. **The page path rule** (finding 1): page documents carry the project's file path.
3. **Recovery renumbering handles** (finding 2) and the unavailable-page placeholder (finding 3):
   acceptable, or should a failed page close itself after the callback returns (a small engine
   addition)?
4. **Locking model** (finding 4): keep `Arc<Mutex>` with the documented discipline, or move to
   passing the owner by `&mut` through callbacks.
5. **Next coverage step:** a parameterised census of the drawing commands most likely to be used
   on schematics (TRIM, EXTEND, FILLET, STRETCH, MIRROR, grip copy, ARRAY), before any limitation
   is lifted.
6. **Product behaviour still deferred:** what ERASE and Delete do by default, the copy meanings
   "another contact" and "another depiction", tag on copy, whether closing the last page closes
   the project, and orthogonal tidy-up when wire ends follow.
