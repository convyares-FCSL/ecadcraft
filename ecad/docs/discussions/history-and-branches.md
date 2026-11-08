# Discussion: history, versions and branches

Status: **Agreed direction** (owner, 2026-11-08). Storage-neutral: how it is stored is
[EEP-0005](../spec/eep-0005.md) (Draft, awaiting a storage bake-off). It supersedes the
folder-project and git-native ideas of [`git.md`](git.md): git is an optional integration,
never the model. It refines the vocabulary of [`revisions.md`](revisions.md) (§2 below).
Raised by: the owner, 2026-11-08. Written by: the overseer, recording the owner's direction.

## 1. Workflow

Onshape-like, all inside ECAD; engineers never see git.

- A **history panel** listing recorded actions, with "show changes" for any of them.
- **Create a version** at the current point.
- **Branch** from any version or point; a **branch graph** shows how branches relate.
- **Switch branch.**
- **Compare** any two points (on one branch or across branches).
- **Merge** a branch into another, with electrical conflicts listed per object: keep mine, take
  theirs, or edit.
- **Issue a revision** (F7, Release).
- **Restore** a past point read-only, or branch from it.

## 2. Words

| Word | Meaning | Shown to users |
| --- | --- | --- |
| **history** | the recorded actions (transactions) of a branch | yes |
| **version** | a named point in a branch's history; it **is** a snapshot of the state there | yes |
| **branch** | an alternative design, with its own history | yes |
| **revision** | an issued release: a version plus release data and its frozen outputs | yes |

This replaces `revisions.md` §3.1, which kept "version" internal: users now see versions *and*
revisions, with the distinct meanings above. Format terms (`container_version`,
`schema_version`, the op `v`) stay internal, as before. Intermediate snapshots written for speed
(`revisions.md` §3.3) are caches, never versions.

## 3. Contracts

1. **History is append-only and immutable.** Nothing recorded is rewritten; the only exception is
   the explicit, confirmed compaction of `revisions.md` §3.4.
2. **Stable identities.** Objects keep their 128-bit IDs across branches and merges
   (EEP-0002 §7.2). The same object on two branches is the same ID.
3. **Each branch is its own independent linear log**, with today's version 1 rules: its own `seq`
   from 1 and its own hash chain (EEP-0002 §6, §8).
   - A branch starts with a **fork transaction** naming the parent branch and the point it
     forked from (`seq` and chain value), and the snapshot at that point (its part hashes).
   - A **merge** is a transaction appended to the target branch. It names the source branch and
     the source head (chain value) and contains the resolved changes as **ordinary ops**, so the
     target branch replays self-contained, without reading the source's log. The source branch
     is then marked merged, or kept open.
4. **`main` is today's log.** A version 1 file converts to a project whose `main` branch holds
   its whole history (EEP-0003 §16.2 converter rules).
5. **Merge is three-way on state**, with the fork point (or the last merge between the two) as
   the common ancestor. It compares the ancestor, mine and theirs **per object and property**,
   then checks the electrical rules (the invariants of EEP-0002 §11.3, and checks such as
   duplicate tags) on the result. A property changed on one side only is taken; a property
   changed differently on both sides is a conflict for the user.
6. **Undo is per branch.** Undoing a merge or a revision issue asks first.
7. **Determinism.** Replaying a branch from its fork snapshot gives the same state every time,
   on every machine (FILE-02).
8. **One self-contained `.ecad`.** Everything needed to open, replay, compare and merge is in the
   file. Caches (intermediate snapshots, indexes) make it faster and are never needed to share
   it. The snapshot a fork starts from is **not** a cache: it is the branch's base, and can be
   checked against the parent's log in the same file.

## 4. Project fields are history

Title, customer, drawing number, status, the revision table, sheet settings and tag settings are
**transactions** in the branch log (`SetProperty` on the project; `SetProjectSettings`), so they
are undoable, branchable and captured by versions.

- Today `SetProperty` can target a component, conductor or cable only (EEP-0002 §7.2,
  `ObjectRef`), and `ProjectSettings` holds the length unit (with brief 018's tag settings
  pending, EEP-0002 §16a). A project target for `SetProperty` and the further settings are
  additive schema changes (EEP-0002, "Compatibility").
- `meta` holds only container facts: format version, file ID, writer build, the branch registry
  and heads, cache bookkeeping.
- Per-user view state (zoom, open pages, panel layout) is not part of the design.

## 5. Local and shared

- **Local first:** branches live inside one file, on one machine.
- **Later:** two copies of a project can be combined, because transactions are content-addressed
  (their chain values) and copies share ancestry: the shared prefix is found by hash, and the
  rest arrives as branches to merge.
- **Live multi-user editing** is a later decision; it would need a server.

## 6. Size

A branch stores only its own transactions after the fork. Snapshot parts are content-addressed
and shared across versions and branches (EEP-0004 §5, EEP-0005), so a file grows with what
differs, not with the number of branches or versions.

## 7. Revisions and branches

Issued revisions are the fixed points the real world relies on. Agreed rules (owner,
2026-11-08):

1. **Revisions are issued only on a release line.** In v0.1 the only release line is `main`.
   Other branches may create versions (snapshots) but not revisions.
2. **Issued revisions are immutable.** No merge, edit or undo changes the state an issued revision
   names. Undoing an issue asks first (§3, contract 6).
3. **A merge lands after the latest revision.** It is appended to the target's current working
   state, after its latest issued revision. A branch forked at Rev A and merged after `main`
   issued Rev B and Rev C changes nothing in B or C; its work first appears in the next issued
   revision, Rev D.
4. **Crossing revisions is allowed, and reported.** The merge compares three states: the common
   ancestor (here the fork point, Rev A), the target now (after Rev C) and the branch head
   (§3, contract 5). The report says "based on Rev A; `main` has since issued Rev B and Rev C",
   and conflicts are framed against those revisions: "Rev B changed -K01 rating to 16 A; this
   branch changed it to 20 A".
5. **Updating a branch first is optional.** "Update the branch from `main`" merges `main` into
   the branch (the same mechanism, reversed), so the branch is based on Rev C and its later merge
   back has fewer and more local conflicts. The common ancestor is then the last merge between
   the two.
6. **Traceability.** The next revision's change list (C → D) records "Merged branch X (based on
   Rev A)", from the merge transaction's source branch and ancestor (EEP-0005 §3.3).
7. **Later, not v0.1: more than one release line.**
   - **Maintenance:** branch from an old revision (Rev B), mark the branch as a release line, and
     issue Rev B.1 on it; optionally merge it forward into `main`.
   - **Customer variants:** each its own release line with its own revision sequence.

   Revision names are **unique across the project** (A, B, B.1, C …), so a revision always names
   one fixed point, whichever line issued it.

## 8. Still open

- Merge conflict UI details: how "edit" is presented, and how a conflict involving a connection
  (a conductor end re-attached on both sides) is shown.
- Whether undoing a merge also re-opens the source branch.
- What "combine two copies" does when both copies changed `main` (it becomes a merge from a
  branch named after the other copy).
