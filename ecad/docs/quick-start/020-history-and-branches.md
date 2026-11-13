# Brief 020: history and branches

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 020b (design `ecad/docs/design/020.md` §4–§6), on `feature/history-merge`, not yet in
`dev`:** a branch is **merged into main** (or into the branch it came from), and a branch is
**updated from main**, through a dialog that previews the changes and lists each conflict with an
explicit choice (nothing preselected; **Keep main for all** / **Keep branch for all**; **Edit**
for text). The merge is one step on main, drawn as a join in the History graph; a plain `u` of a
merge asks first (**Undo merge**), and undoing it reopens the branch. The branch bar's **Merge
into main…** and **Update from main…** buttons work now (they were disabled in 020a); so do
Revisions' **Merge into main…**, the History tab's **⋯** entries and a Now row's right-click.
A branch's first save writes its first snapshot, so switching to it costs about an open. The
History graph reads 200 rows and the next 200 when scrolled to the end; a version's details name
the devices it changed (`-K05 added`); **F2** renames the branch shown. Section 5J steps 3, 5 and
9–11 describe it.
- *Automated checks:* `ecad/crates/ecad-file/tests/all/merge_store_contracts.rs` (the merge record,
  the merge index and the merged status through merge, undo and redo across saves, the first-save
  snapshot), `ecad/apps/ecad-mcp/tests/all/merge_contracts.rs` (merge, update, a conflict refused
  without a choice and merged with one, undo merge, writes off), the merge engine's unit tests in
  `ecad/crates/ecad-model/src/merge.rs`, and `ecad/apps/ecad-trial/tests/all/merge_ui.rs` (the
  branch bar's Merge into main…, the dialog with one conflict, Merge, the join, Undo merge asked).
- *Agent-observed:* the worker looked at the headless screenshots `020-merge.png` and
  `020-undomerge.png`; no agent has clicked through the live window.
- *Owner-tried:* not yet.

**Brief 020a (design `ecad/docs/design/020.md`), merged into `dev` (PR #48):** the
project's history is a **History** tab in the PROJECT panel: a graph of the main line with its
versions and issued revisions, and branches drawn from where they start. Select a row to see it,
**Show** it read-only, **Restore** it (one new step, undoable), make a **New version**, and work
on **branches** (create, rename, switch with **Save and switch**, archive, reopen). A project
reopens on the branch you last used; the window title and a branch bar say which branch is shown
once there is more than one. Section 5J describes it.
- *Automated checks:* `ecad/crates/ecad-file/tests/all/branches_contracts.rs` (branch rows,
  switching, reopening on the last branch, snapshots on a branch, deep branches, Verify, readers
  ignoring unknown optional fields), `ecad/apps/ecad-mcp/tests/all/history_contracts.rs` (every
  history and branch command through MCP, refused with writes off), and the unit tests of the
  graph's order and lanes in `ecad/crates/ecad-cadcraft/src/history.rs`.
- *Agent-observed:* not yet clicked through by an agent in the live window.
- *Owner-tried:* not yet.
