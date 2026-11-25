# History, versions, branches and merge

Every change to a project is a step in its history, and the history is kept in the project file.
You can look at any earlier state, keep a named **version**, try an idea on a **branch**, and
**merge** it back. Nothing in the history is ever deleted: going back is a new step that Undo takes
back. ECAD uses the words version, revision, branch and merge (see the
[glossary](../reference/glossary.md)); issuing a **revision** is in [releases.md](releases.md).

Branches and versions live in the project file: a project that was never saved says
`Branches live in the project file. Save it first.` and offers **Save As…**.

## Open the History panel

Press **History** on the tool strip at the left of the Draw workspace (the third panel button, after
Book and Devices). Its pin keeps it docked; unpinned it floats over the drawing and closes when you
start drawing. The bottom bar's revision pill opens it too.

From the top down the panel shows:

- **The pending revision**: `Rev B* pending`, which revision it follows, what the checks found, a
  **Release B…** button (see [releases.md](releases.md)) and **Compare** (not available yet).
- **The agent**, while one is connected: who is writing on which branch, **Pause** or **Resume**,
  and disconnect (see [agent-access.md](agent-access.md)).
- **Branches**: the branch shown first ("you are here"), the other open branches with how many
  steps ahead they are and **Merge**, and **New branch**. Tick **Show merged and archived** to list
  those too, greyed.
- **Changes**: the branch shown's steps, newest first, each with its author, time and branch.
  **Show earlier changes** reads more.
- **Versions**: named versions and issued revisions. **Name this one** keeps the project as it is
  now as a version. **Show older versions** reads more.

## Who made each change

Every step has its author: **You** for the person at the computer, or the agent's name in green. No
name is stored for a person. An agent's consecutive steps fold into one row
(`claude-code · 14 changes · 20:31–20:44`); click it to see its steps. While that agent is still
connected the row offers **Undo session**; once the session has ended its steps undo one by one like
any others. Selecting a change outlines what it changed on the page, in blue, with its author.

## Keep a version

**Name this one** (in Versions) opens **New version**: type a **Name** and, if you like, a
**Description**, then **Create**. The version appears in the list; Undo takes it away and Redo
brings it back. Save to keep it. Versions are made at the current state only.

Typed: `version {"name":"Before change"}`,
`version {"name":"Before change","description":"before the cable rework"}`.

## Look at an earlier state

Select a change or a version. Its details show what it is, on which branch, when and by whom, and
(for a version) the devices it changed. Then:

- **Show** opens that state read-only. A bar names it with **Back to current**; the Book lists that
  state's pages, and clicking one opens a read-only preview.
- **Branch from here…** starts a branch at that point (below).
- **Restore…** makes the project on the branch shown as it was at that point. The question says how
  many objects change and on which pages, and that nothing is deleted from the history: it is one
  new step, and Undo takes it back.

Typed: `history {}` (more rows: `history {"limit":400}`), `steps {"limit":10}`,
`pointinfo {"version":"Before change"}`, `showpoint {"version":"Before change"}`, `closepoint {}`,
`restore {"version":"Before change","preview":true}` (the question only, nothing recorded),
`restore {"version":"Before change"}`.

## Work on a branch

- **New branch** (in Branches) or **Branch from here…** (on a selected point): type a **Name**,
  keep **Switch to the new branch** ticked, **Create**. Once a project has more than one branch, the
  window title and the panel's branch bar name the branch you are on.
- **Switch**: double-click a branch, or right-click it ▸ **Switch to "…"**. With unsaved changes it
  asks first: **Save and switch** saves them, then shows the other branch. Undo works on each
  branch's own steps.
- **Rename**: right-click ▸ **Rename branch…**, or press **F2** with the History panel shown.
- **Archive**: right-click ▸ **Archive branch…**. An archived branch is hidden until **Show merged
  and archived**; select it and **Reopen branch** to bring it back.
- A project reopens on the branch you last used.

On a branch other than main, a **branch bar** at the top of the left panel names it and offers
**Merge into main…** and **Update from main…**.

Revisions are issued on main only: on a branch, the Revisions window's **Issue Rev …** is disabled
with `Revisions are issued on main. Merge "<branch>" into main first.` and the window offers
**Merge into main…** and **Switch to main**.

Typed: `branch {"name":"Option B","from":{"version":"Before change"}}`,
`switchbranch {"branch":"main","save":true}` (without `save` it is refused while there are unsaved
changes), `renamebranch {"branch":"Option B","name":"Option C"}`,
`archivebranch {"branch":"Option C"}`, `reopenbranch {"branch":"Option C"}` (when another open
branch took the name, add `"name":"Option C (2)"`), `branches {}`.

## Merge a branch into main

1. On the branch, click **Merge into main…** in the branch bar (or **Merge** on the branch's row,
   or right-click it ▸ **Merge into main…**).
2. The dialog `MERGE "<branch>" INTO main` says that it switches to main, saving the branch first,
   and merges into it. It shows where the branch started (and any revision main issued since), a
   count such as `2 changes from the branch · page 3 · 0 conflicts`, and each change
   (`~ -K01 · Description "…" → "Pump 1"`, `+ Wire … added`). **Close branch** is ticked.
3. Nothing is written yet: **Cancel** leaves everything as it was.
4. **Merge**. The project shows main, and the command line says
   `"<branch>" was saved; now on main. Merged "<branch>" into main (2 changes). Save to keep it.`
   The history has a row `Merged "<branch>"`. After **Save**, a closed branch shows only with
   **Show merged and archived**.

The merge is one step on main. A revision issued after it includes the merged changes.

## Choose for each conflict

When main and the branch changed the same value, the dialog lists each conflict under
`CONFLICTS (n)`: what it is, the value on main and on the branch, and the choices **Keep main**,
**Keep branch** or **Edit** (type another value). Some conflicts offer other choices, such as
**Bring it back**, **Leave unconnected**, or renumbering one side's device. Nothing is preselected:
**Merge** stays disabled until every conflict has a choice. **Keep main for all** and **Keep branch
for all** answer every open conflict at once.

## Update a branch from main

On a branch, after main has changed, click **Update from main…** in the branch bar. The dialog
`UPDATE "<branch>" FROM main` says it brings main's changes since the branch started (or since the
last update) into this branch, and that issued revisions are not changed. The sides are named `main`
and `this branch`; there is no Close box. **Update** records one step on the branch:
`Updated "<branch>" from main (…). Save to keep it.` A later merge into main compares against that
update, so main's changes are not conflicts again.

## Undo a merge

Press `u` (or Ctrl+Z) right after a merge: it asks `Undo the merge?`, naming the branch and the
number of changes; if the merge closed the branch, the branch is reopened. **Undo merge** takes it
back; Redo merges again.

Typed: `merge {"from":"Option B","preview":true}` (the dialog's preview as JSON; records nothing and
works with an agent's writes off), `merge {"from":"Option B"}` (into its parent: switches there
first, saving; refused with each conflict's ID while one has no choice),
`merge {"from":"Option C","choices":{"<id>":"branch"}}` (each choice is `main`, `branch`, `loose`,
`bringback`, `renumber_branch`, `renumber_main`, `one` or `{"value":"text"}`; `"close":false`
keeps the branch open), `merge {"from":"main"}` on a branch (update from main), `undomerge {}`.

## Limits

- A branch merges into, and updates from, its parent only.
- The merge dialog has no preview of an object's page as on main or on the branch, and it does not
  count the new Checks rows: look at the Checks tab after the merge.
- A redo of a merge whose undo was saved in an earlier session merges again but leaves the branch
  open; close it from History.
- **Compare** (revisions and versions) and the Changes filters other than **All** are not available
  yet.
- Times are shown in local time; the file keeps UTC.
