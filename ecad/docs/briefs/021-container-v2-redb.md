# Brief 021: Container v2 on redb

Status: **Approved** (owner, 2026-11-08: "Yes go ahead"; earlier "Yes,
then brief 021")
Branch: `feature/container-v2` (worktree `C:\dev\ecad-features\container-v2`), from `feature/sheets`
(016 + 018 + 019), so it stores the full current model. It is rebased as those merge into `dev`.
Designer: no (no UI beyond the debugger views and the open/convert prompt) · Size: L
Specs: **EEP-0005** (normative; on `feature/developer-docs`, `ecad/docs/spec/eep-0005.md`),
`docs/discussions/history-and-branches.md`, `storage-engine-decision.md` and `revisions.md`, plus
EEP-0002 (v1) for the converter.

## Goal

`.ecad` becomes a single **redb** file holding the agreed tables. Saving and opening go from seconds
to milliseconds on large projects, and the file is ready for versions, branches and assets. Every
v1 file converts, and the file's structure is easy to see with our own tools.

## Scope

1. **The container** (`ecad-store` / `ecad-file`), exactly as EEP-0005 says:
   - one redb file;
   - tables `meta`, `branches`, `transactions` keyed by `(branch_id, sequence)`, `versions`,
     `parts` (content hash) and `assets` (content hash);
   - Zstandard for ops and parts; assets stored as they are; the large shared model part split
     into fixed-size chunks;
   - the redb version pinned.
2. **The history model, normative:**
   - each branch is its own logical, append-only log;
   - `main` starts from the project root;
   - a branch's fork record names an exact transaction in its parent;
   - ancestry is referenced, never copied;
   - **append-only is enforced by ECAD's API.**

   This brief creates and uses **`main` only**. Creating, switching and merging branches is a later
   brief, but the tables and fork records are in place and tested.
3. **Versions as snapshots:** the existing `NameVersion` and issued revisions (019) write a snapshot
   (parts) at that point, as `revisions.md` agrees. Intermediate automatic snapshots every N
   transactions are a setting with a sensible default.
4. **Save:** one redb write transaction holds the new transaction rows, any new parts and the
   updated head. The cost is proportional to the change, never to the file size.
5. **Open:** read only what's needed: `meta`, `branches`, then the active branch's latest snapshot
   (the shown page's parts first, others on demand), then the tail after it. Never other branches,
   old versions, the full log or assets. Use quick-repair on open after a crash. Undo past the
   snapshot reads older rows on demand. Keep the lock-file behaviour (read-only for a second
   opener).
6. **Converter:**
   - opening a v1 `.ecad` **offers** to convert, never silently;
   - it writes a new v2 file and leaves the v1 file untouched;
   - v1's history becomes `main`, with a link record holding the v1 file's final hash;
   - it verifies by replaying both and comparing state, failing loudly on any difference;
   - also `ecad upgrade <file>` on the command line;
   - v1 reading is kept.
7. **Seeing the file** (the owner's request, the same rule as the format spec):
   - `ecadinspect` on v2: `tables` (counts and sizes), `branches` (tree, fork points, heads),
     `log <branch> --last N` (decompressed, readable), `versions`, `part <hash>`, and
     **`export-jsonl`** (the full history as readable v1-style JSONL);
   - the in-app debugger gets **Storage** and **Branches** views;
   - the HTML inspector reads the export.
8. **Bench and tests:**
   - `cargo xtask bench` gains v2 columns: save per edit, open, open after a crash, and file size.
     Compare against the bake-off and v1 numbers; don't re-run old benches.
   - **The crash, torn-write and power-loss-ordering tests are permanent,** in the slow set.
   - The rest of EEP-0005's test requirements: Linux, cold-cache open, and real assets.

## Later (owner, 2026-11-08), not in this brief

- **HTML inspector: a raw/readable toggle.** For any record (a transaction row, chunk or asset),
  switch between the human-readable decoded view and the raw stored form (compressed bytes as hex,
  with the table, key, size and compression), and back, read-only, never changing the file. To keep
  this cheap later, `export-jsonl` in this brief should be able to include each record's raw stored
  bytes alongside the decoded form (an option such as `--with-raw`).
- **HTML inspector opens `.ecad` v2 directly via the wasm reader** (read-only), with the
  raw/readable toggle (overseer, 2026-11-09). This brief keeps the way open: the v2 reader reads
  through redb's `StorageBackend` (`Store::from_bytes` over an in-memory buffer), and `ecad-file`
  with redb `=4.5.0` and `ruzstd` builds for `wasm32-unknown-unknown` (`cargo xtask wasm`).

## Not in this brief

- Branch create, switch and merge UI.
- The History tab (020).
- Defrag / Compact UI: compaction exists, with no user action yet.
- Live multi-user editing.

## Rules

- **Format and op decisions are the overseer's:** stop and ask, never decide. EEP-0005 is the
  contract; any deviation is proposed, not done.
- `ecad/CLAUDE.md`: no lazy defaults (open and save scale with the change, not the file); own
  build folder; `test-changed`.
- The store is a **risky area**: a tester session reviews the crash and recovery tests before the
  PR.

## Acceptance criteria

1. The demo and the large bench project convert from v1, replay-verified identical, and open in
   v2.
2. On the large tier, save per edit and open are in the bake-off's redb range (about 1–1.4 ms per
   save; open under about 100 ms). Open time stays flat as history grows tenfold.
3. Crash, torn-write and power-loss-ordering tests: 0 failures over the agreed rounds.
4. `ecadinspect` shows the tables, branches, log, versions and parts; `export-jsonl` round-trips
   into a v1-readable history.
5. Old v1 files still open, with the convert prompt.
6. EEP-0002, EEP-0005 and the quick start are updated to the as-built facts.

## Needs owner check

- A quick look at this brief before the worker starts.
- A trial: open `demo.ecad` (v1), accept the conversion, edit, save, and check the save time; kill
  the app mid-edit and reopen; run `ecadinspect` on the new file.
