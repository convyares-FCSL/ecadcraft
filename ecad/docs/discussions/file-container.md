# Discussion: how the `.ecad` file grows (log, snapshots, parts)

Status: **For discussion** with the architect (Claude, ECAD Project). Nothing here is decided.
Raised by: the owner, 2026-11-07. Written by: the overseer, with its own view in section 4.

## 1. The owner's idea

In future the project may become a **zip of parts**: the main part is the log of changes, and the
snapshots are separate parts. Dividing the project into a part is then concrete and deliberate,
not implicit.

## 2. Where we are

- `.ecad` is one append-only log of transactions (CRC-framed records). Opening replays the whole
  log; saving appends and flushes. This is a fixed principle (`docs/concept-spec.md`: "append-only
  operation log, not SQLite or any database").
- Already designed, not built: **checkpoints** (stage 4) as a cache of materialised state inside
  the file, so opening reads the latest checkpoint and replays only the tail; **compaction** to
  latest checkpoint plus named versions; **libraries** in separate `.ecadlib` files (stage 3).
- Pages are not containers: every drawing item names its page, and objects such as a cable span
  pages. Splitting by page is a storage choice, not a model change.
- Brief 012 adds an on-demand load benchmark so this is decided from numbers.

## 3. What problem would parts solve?

The word "parts" covers four different problems, and each has a different best answer:

| Problem | Symptom | Natural fix |
| --- | --- | --- |
| A. Open time grows with history | Slow open on old projects | Checkpoints (snapshot + tail) |
| B. File size grows with history | Large files to send | Compaction, or a packed export |
| C. Huge projects, only part needed | Slow open even when compact | Load on demand, split **by content** (pages, areas) |
| D. Several people on one project | Conflicts, no merging | Split by content plus a merge story |

The owner's idea, snapshots as parts, answers A and B. It doesn't answer C or D, which split
**by content**, not by time.

## 4. The overseer's view (for the architect to challenge)

1. **Agree: snapshots should be explicit, named things.** A checkpoint should be a distinct,
   versioned unit, listed in a manifest, that can be verified on its own (replay equals snapshot),
   deleted without losing anything, and inspected in the debugger. That is the "concrete and
   deliberate" part, and it doesn't require zip.
2. **Concern: zip is a poor *live* format for an append-only log.** Zip keeps its directory at the
   end of the file, so every save must either rewrite the directory (not crash-safe without the
   whole temp-file-and-rename dance) or rewrite the whole archive (save cost grows with project
   size). Today a save appends a few hundred bytes and flushes, and a crash at any point is
   recoverable; that is one of the strongest properties we have, and brief 002 spent effort on it.
3. **Suggested shape:** keep the live `.ecad` as the single append-only file, and add checkpoints as
   typed records with a small manifest record (A). Define a **package** format (zip) for sending,
   archiving and handing over: log, latest checkpoint, the libraries used and exports, all stamped
   with one revision (B, and it matches concept-spec's "exports to one folder or zip"). If C or D
   ever arrives, revisit a container (folder bundle or zip) then, with the benchmark's numbers.
4. **Alternative worth weighing:** a **project folder** (bundle) as the live format: `log.ecad`,
   `checkpoints/*.snap`, `libraries/`, each written atomically; zip only for transport (EPLAN-like).
   Parts are real files, but one project is no longer one file to copy, lock and back up.

## 4a. Option C, from the owner's follow-up: index plus log plus snapshot parts, append-only

The owner's model, a struct of an index, the live log and an array of snapshots, is a good logical
view. On disk only the end of a file can grow, so an index at the front can't grow without
overwriting bytes in place: either a reserved region or two alternating slots with a CRC, as LMDB
does. Putting **each new index at the end** keeps every write an append, as PDF incremental
updates and an appended zip do:

```
[header][tx 1 … 1000][snapshot parts @1000][index v1][tx 1001 … 1050][index v2]   ← read the last valid index
```

- **The log is never duplicated.** It is one sequence of transactions stored in segments, and
  snapshot and index records sit between them. Full history reads the `tx` records in order and
  skips the rest.
- **A snapshot is materialised state, not history.** It is a cache: delete it and nothing is lost.
  Opening reads the last index, loads the snapshot and replays only the tail, so open time follows
  the project's size, not the length of its history.
- **Parts make snapshots incremental.** Split the state into parts, for example one per page plus
  one for project-wide objects (components, cables, settings). A new snapshot writes only the
  parts that changed, and the index points at the newest copy of each, much as git does with
  trees. This is where the owner's "concrete and deliberate" division lives.
- **Snapshot when the tail costs more than the snapshot,** not on every save.
- **Compaction** is the only rewrite: write a new file, then swap it in with an atomic rename,
  keeping the full log, the latest parts and the named versions.
- **Hash chain:** transactions are chained (`chain.rs`). Each snapshot records the chain hash at its
  point, so opening checks only the tail, and a full-history check becomes an on-demand verify
  (the debugger's Verify replay and Verify file already exist).
- **The wrapper is still open:** a real zip (7-Zip can open it, and parts are zip entries) or our own
  CRC framing (one integrity scheme). Either can be append-only.

The overseer recommends C with the index at the end. The live format and the transport package
could then become the same thing.

## 4b. First benchmark result (brief 012, small tier, owner's machine, 2026-11-07)

| History | File | Transactions | Open |
| --- | --- | --- | --- |
| lean | 0.86 MB | 678 | 230 ms |
| churned | 3.24 MB | 3 386 | 1 050 ms |

Opening takes roughly the number of transactions times the size of the document: replay copies
and validates the whole document after every transaction. That is a replay inefficiency to fix
whatever the container is (validate once at the end, or incrementally). It also means the first
page waits almost entirely on open. Snapshots hide it, but they shouldn't be the only fix.

## 4c. Outside review (owner, 2026-11-07) and the overseer's check against the code

| Point | Already covered | Gap |
| --- | --- | --- |
| Model-first (logical model separate from drawing) | Yes, the core of the design | — |
| Orphaned representations | DOC-05 and the invariants refuse dangling references; an erased reference end gives the red `/?` | Confirm the cascade rule when a conductor or component is deleted |
| T-junctions / multi-drop | — | **Undecided.** Proposed: a T resolves to two point-to-point conductors (daisy chain on a terminal), keeping the two-end contract |
| Silent replay drift | Hash chain; Verify replay | Each snapshot is checked to equal replay when written (option C) |
| Repeated nudges grow the log | — | Merge repeated moves of the same selection into one transaction (small brief later) |
| Git and multi-user merging | — | Question 3 (problem D) |
| Schema evolution | `schema_version` in the header; ops versioned and upcast on replay (`v1.rs`, concept spec) | Snapshots and compaction always write the latest schema |
| Libraries outside the project | Embedded copy with origin; updates explicit (concept spec, brief 006) | — |
| Binary attachments (PDF, DXF, images) | — | Never inside the log: content-addressed parts, which is where a real zip wrapper earns its place |
| Benchmark coverage | Open, nets, first page, one save | One edit on a loaded large project; cable references once brief 010 is merged |

## 5. Questions for the architect

1. Is "snapshots as explicit parts" satisfied by checkpoint records plus a manifest inside one file,
   or should parts be separate files or zip entries?
2. Should the live format and the transport/archive format be different (append-only file live, zip
   package to send), or the same?
3. Which of problems A to D do we expect within the next year, and does D (several people) need
   designing for now, since it is the one that would force splitting by content?
4. What numbers from brief 012 should trigger stage 4 (open time and file size budgets)?
5. Does any of this change a fixed principle (one append-only log, not a database), and if so who
   approves it?
6. Option C: index at the end (pure append) or at the front (two slots, rewritten in place)? A real
   zip wrapper or our own framing?
7. How should the state be split into snapshot parts: per page plus project-wide, or finer?
8. Should replay's whole-document validation per transaction be fixed now, as its own brief, since
   it dominates open time before any snapshot exists?
9. T-junctions: confirm that a T always resolves to two point-to-point conductors on a shared
   terminal.
10. Attachments: content-addressed parts inside the container, or a sidecar folder?
