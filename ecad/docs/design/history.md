# History, versions and branches

How a project keeps its past, why it is shaped that way, and the rules that keep merges safe. How to use the
History tab is in [../user/history.md](../user/history.md); how revisions are issued and released is in
[../user/releases.md](../user/releases.md); how it is stored is in
[../reference/file-format.md](../reference/file-format.md).

## The model in one paragraph

Everything that happens to a project is a step (a transaction) in the history of a branch. History is
append-only: nothing recorded is rewritten. A **version** is a name for a point in a branch's history. A
**revision** is an issued release: a named point plus its release data. A **branch** is an alternative design
with its own history. A **merge** brings one branch's work into another as one new step. It all lives inside
the one `.ecad` file; people never see git.

Why: engineers need to try an alternative without copying files, go back to what was issued, and see who
changed what. One self-contained file, with its whole history, is what they share and archive.

## Words

| Word | Meaning |
| --- | --- |
| history | the steps of a branch, newest first in the History tab |
| version | a name a person gives to the newest point of the branch they are on |
| revision | an issued release, numbered by the project's revision scheme |
| branch | an alternative design, forked from a point on its parent |
| merge | one step on the target branch that brings the source branch's changes |

Versions and revisions are **two kinds of named point**, keyed by kind and name (`PointName`), never told apart
by spelling. So a version `B` and revision `B` can both exist. Automatic snapshots that make opening faster
are caches, never versions; the History tab hides them unless its toggle shows them, faint, as
`Auto · <time>`.

## Contracts

1. **History is append-only.** A recorded step is never rewritten.
2. **Stable identities.** An object keeps its ID on every branch and through every merge. The same object on
   two branches has the same ID, which is what makes a per-object merge possible.
3. **Each branch owns its own log.** `main` starts at the project root. Every other branch starts from a fixed
   point on its parent. Ancestral history is referenced, never copied: a branch is its parent at the fork point
   plus its own steps, and later parent steps do not affect it.
4. **A merge is one step of ordinary ops on the target.** It names the source branch, the source head and the
   ancestor, so the target replays on its own without reading the source's log. The choices made in the merge
   dialog are not stored separately: the ops are the result.
5. **Determinism.** Replaying a branch gives the same state every time, on every machine. Replay applies the
   recorded ops; nothing is recomputed.
6. **Undo is per branch.** Undoing a merge or an issue asks first (`undomerge`, `undoissue`).
7. **One self-contained file.** Everything needed to open, replay, compare and merge is in the `.ecad` file.
   Snapshots only make opening faster; none is needed to share the file.
8. **Open reads only what is needed:** the active branch's latest snapshot and the steps after it. Other
   branches, older points and full histories are read when used. Save writes only what is new.
9. **Every named point has a snapshot.** The save that writes a version or an issued revision writes a
   snapshot at that point, so showing it, or comparing two named points, is state to state and never replays a
   noisy log.
10. **History is never dropped because a revision was issued.** What large organisations need is
    traceability (what a revision contained, who issued it, what changed since), not undo past a release: a
    change after release is the next revision.

## Project facts are history too

Title, customer, the revision table, sheet settings and tag settings are changed by steps like any drawing
change, so they are undoable, branch with the design and are captured by versions. Per-person view state (zoom,
open pages, panel layout) is not part of the project.

## Versions

- A version names the newest point of the branch shown (`version`). Naming an older point is not offered:
  branch from it instead.
- A version name is unique on its branch. Version names are branch-local: a merge keeps the target's.
- Creating a version is one step and is undone by a plain undo, without asking.

## Revisions and the release line

Issued revisions are the fixed points the real world relies on.

- **Revisions are issued on the release line only.** Today the only release line is `main`. On another branch
  the pending revision's fields are read-only and Issue is refused with the way forward ("Merge … into main
  first").
- **A revision is typed and follows the project's revision scheme** (`RevScheme`): letters (`A`, `B`…, with an
  optional dash for the first issue) or numbers (`P01`, `P02`…). The next rev comes from the scheme only; a rev
  outside it is refused, naming the problem. The scheme cannot change once anything is issued, because order
  across schemes has no meaning.
- **Issued revisions never change.** No merge, edit or undo changes the state an issued revision names.
  Undoing an issue asks first.
- **Finalise and the issue are one step**, so one undo takes back both.

## Branches

- A branch forks from a point on its parent and merges into, and updates from, its parent only. Merging
  between sibling branches is not offered: it keeps the history a tree that people can read.
- Branch actions (create, rename, switch, archive, reopen) are written to the file at once, after saving the
  branch shown, and are not undoable steps; each has its inverse action. Why: a branch is structure around the
  history, not a change to the design.
- `main` is the release line and cannot be archived.
- A project opens on the branch last used. The window title names the branch once there is more than one.
- Restore is offered only on the current branch's own line. It writes one ordinary step, labelled
  `Restore "…"`, whose ops are the difference between the live state and the point's, so its cost follows what
  differs. Version names and revision fields are never restored: they are history, not design.

## How a merge works

**Sides.** *Merge into main* runs on `main`, switching there first and saying so. *Update from main* runs on
the branch: the same mechanism, reversed, so a branch can catch up and its later merge back has fewer, more
local conflicts. **Close branch after merging** is ticked by default.

**Ancestor.** The fork point, or the newest live merge between the two branches in either direction, read from
the merge index, never by scanning the logs.

**What is compared.** Three states, each on a copy: the ancestor, the target and the source. Only objects that
either side's steps named since the ancestor are compared, so the cost follows what changed, not the size of
the project. Per object and per property:

- changed on one side only: that side is taken;
- changed alike on both: kept;
- changed differently: a **value conflict**;
- removed on one side and changed on the other: a **removed conflict**;
- added on one side: added.

**Rules the result must keep** are conflicts too, each with a choice that satisfies it, so the merge step
always passes the document's own checks:

- something kept that refers to something removed: bring it back, or leave it unconnected;
- two devices with one tag: renumber the source's device (the suggested choice), renumber the target's, or
  make them one device where that is possible;
- two pages with one number: renumber one;
- two sibling folders with one name: rename one;
- page or folder order changed differently on both sides;
- the same revision issued on both branches.

Provisional tags added on both sides are not conflicts: the source's gets the next provisional number.

**Nothing is preselected.** Every conflict needs an explicit choice (or *Keep main for all* / *Keep branch for
all*), and every conflict offers at least two ways out, so a merge is never a dead end. A wire end attached
differently on both sides always offers **Leave unconnected**; the Checks tab then lists the open end.

**Not merged:** version names (the target's are kept); revision fields are never merged into the release line,
and an update from `main` brings its issued revisions into the branch unchanged; the tag high-water mark is the
larger of the two.

**Undo.** Undoing a merge asks first, and reopens the source branch if the merge closed it.

## Why it scales

- The History graph is built from the branch rows, the named-point rows and one step row per node shown (its
  time, author and label). It never scans a log, and reads one page of rows at a time.
- Showing a point reads its nearest snapshot plus at most the snapshot interval of steps.
- Switching branch costs about an open.
- A merge reads the two stretches since the ancestor and three states, not the whole project.
- A branch stores only its own steps after the fork; snapshot chunks are content-addressed and shared, so a
  file grows with what differs, not with the number of branches or versions.

## Later

- More than one release line (maintenance branches, customer variants), with revision names unique across the
  project.
- Combining two copies of one project: the shared history is found by hash and the rest arrives as branches to
  merge.
- Live multi-user editing would need a server and is a separate decision.

Background: [archive/designs/](../archive/designs/) and [archive/discussions/](../archive/discussions/) hold
the original history design and discussion.
