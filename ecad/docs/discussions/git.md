# Discussion: ECAD projects in git

Status: **Superseded in part** (owner, 2026-11-08). Git is an **optional integration or
export, never the model**: history, versions, branches and merges live inside ECAD
([`history-and-branches.md`](history-and-branches.md)), stored as
[EEP-0005](../spec/eep-0005.md) describes. What still stands: §1 (why a raw `.ecad` fights git)
and §2.1 (a `git diff` driver built on the shared state-diff engine), for teams that keep `.ecad`
files in git. §2.2's merge driver, if built, calls ECAD's own three-way merge
(`history-and-branches.md` §3.5). §3 (folder projects) is **dropped**: the project is one
self-contained file, and a folder is no longer an unpacked form of it.
Related: [`revisions.md`](revisions.md), [EEP-0002](../spec/eep-0002.md),
[EEP-0003](../spec/eep-0003.md), [EEP-0004](../spec/eep-0004.md).
Raised by: the owner, 2026-11-08. Written by: the overseer, recording the owner's reasoning.

## 1. Why today's `.ecad` fights git

It is more than "one file":

- **The append tail.** Every edit on every branch lands at the end of the file, so two branches
  that edited page 1 and page 50 still conflict on the same lines.
- **`seq` and the hash chain.** Both branches use the same next `seq`, and each transaction's
  `prev` hashes its predecessor (EEP-0002 §6, §8), so splicing lines breaks the chain. Version 2
  has the same problem with seals (EEP-0003 §7.1).
- **Preconditions.** Ops carry exact `before` values, and invariants are checked per transaction
  (EEP-0002 §7.4, §11), so "their" transactions cannot be replayed blindly onto "our" state.
- **Diffs.** Raw appended ops are unreadable as a diff; a version 2 binary file is opaque.
- **Churn.** Every intermediate edit enters git history, not just the result.

One thing already helps: IDs are 128-bit values allocated per session (EEP-0002 §7.2), so two
branches never mint the same ID for different objects.

## 2. Layer A: drivers for `.ecad`

### 2.1 `git diff`: the shared state-diff engine

- The diff driver (`textconv` in `.gitattributes`) uses **the same state-diff engine** as
  revision comparison ([`revisions.md`](revisions.md) §3.2). It compares materialised document
  or chunk states, never raw ops.
- So `git diff`, even on version 2 binary files, prints engineering changes:
  "Page 4: added component -K01", "Page 12: -F01 property rating 10A → 16A", "wire re-attached".
- With snapshots (EEP-0004), the engine skips every chunk whose `state_hash` is unchanged.

`[OPEN: how the driver is invoked. (a) the `ecad` command-line tool (EEP-0003 §16.2), for example
`ecad diff --textconv <file>` (recommended: one tool for upgrade, diff and merge); (b) a separate
small binary.]`

### 2.2 `git merge`: rebasing transactions

After the shared prefix (the longest common run of transactions, found by matching chain
values), the merge driver rebases "their" transactions onto "our" state:

1. re-sequence `seq`;
2. recompute the chain: `prev` in version 1, seals in version 2;
3. check each op's `before` precondition and the touched invariants against the merged state,
   exactly as replay does (EEP-0002 §11).

- Branches that edited different pages or objects pass automatically, which is most real team
  work.
- When both changed the same object or property, the driver reports a **domain-level conflict**
  naming the object ID (and its tag) and the property, not a text conflict, and writes nothing.
- The merged file is written and verified like a converter's output (EEP-0003 §16.2): replayed,
  and refused if it does not open clean.

Points the rebase must handle:

- **Undo and redo.** Their undo targets the top of *their* undo stack, which is no longer the top
  once our transactions come first (EEP-0002 §11.4).
  `[OPEN: (a) rebase their undo and redo transactions as plain edits with the same concrete ops
  (recommended: the ops already say exactly what changed); (b) refuse to merge a branch whose
  history after the shared prefix contains an undo or redo.]`
- **Named revisions.** `NameVersion { at }` names a `seq`, which re-sequencing changes; the
  driver remaps `at`. Two branches issuing a revision with the same name conflict at domain level.
- **Snapshots** after the shared prefix are caches and are rebuilt, not merged.

`[OPEN: conflict resolution. (a) the driver fails with a conflict report and the user re-applies
the change in the app on top of the merged state (recommended for Layer A); (b) a merge view in
the app that offers "ours", "theirs" or a value per conflicting property.]`

### 2.3 Caveat: web pull requests

Local `.gitattributes` drivers work in the git CLI and in editors (VS Code and others). GitHub
and GitLab web pull requests do not run custom drivers: they show `.ecad` as an unreadable text
or binary change and cannot merge it. Two options:

- **Layer B**: per-page and per-object files that plain git diffs and merges (§3);
- **a committed review summary**: a human-readable file (for example `<name>.ecad.changes.md`)
  regenerated on commit by a hook from the state-diff engine, so reviewers read the change on the
  web.

`[OPEN: (a) the summary file as a stopgap until Layer B, generated by a pre-commit hook the
project opts into (recommended: cheap, and it serves teams before stage 4); (b) wait for
Layer B.]`

## 3. Layer B: folder projects (`.ecaddir/`) (dropped, 2026-11-08)

> Kept for the record. Superseded by `history-and-branches.md`: branches and merges are
> inside ECAD, and the `.ecad` file is never a folder. An export of a project as files for
> external tools remains possible, as an export only.

Each EEP-0004 chunk becomes a deterministic, canonical file in a project folder:

- **Pages.** Each `"page:<PageId>"` chunk (the page's items and the representations on it,
  including their connection points) is its own file, so page edits merge as separate files on
  GitHub with no driver.
- **The project chunk** (components, terminals, conductors, cables, definitions, layers, settings)
  is written as **one canonical line per object, sorted by ID**, or split into one file per
  object table. Two branches that add different components or wires then auto-merge with plain
  git line merging.
  `[OPEN: (a) one file per object table, one line per object (recommended: small files, and
  conflicts name the table); (b) one project file, one line per object.]`
- **History.** git is the history and the collaboration log. A local, git-ignored
  `.ecad-session` log gives the engineer undo and redo between commits.
- **Single file when needed.** For email or release (F7), ECAD packs the folder into one
  self-contained `.ecad`. The converter works both ways.
- **Ignored files.** `.lock`, `.tmp-*`, `.tail-*` and `.ecad-session` are always git-ignored
  (EEP-0002 §13).
- **Integrity.** Each file carries its chunk's `state_hash` (EEP-0004 §5); ECAD checks the
  folder's invariants on open, after any merge, and refuses to save over a folder that does not
  pass, with a domain-level report.

`[OPEN: the folder files' encoding. (a) canonical JSON lines, as EEP-0002 §4 (recommended: git
line merges and web diffs need text); (b) version 2 binary per chunk, with the diff driver.]`

## 4. Rollout (as first agreed; Layer B no longer applies)

| When | What |
| --- | --- |
| **When a second person joins** | Layer A: the diff driver and the merge driver for `.ecad`. |
| **Earlier, if teams need web review** | The committed review summary (§2.3). |
| **Stage 4 (snapshots, EEP-0004)** | Layer B designed together with snapshot chunks: the folder layout is the chunks' layout. |
| **With Layer B** | Web review without drivers; the summary file becomes optional. |
