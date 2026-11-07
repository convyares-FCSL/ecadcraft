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
