# Brief 020: History and Branches

Status: **Approved** (owner, 2026-11-10: "lets do History and branches")
Branch: `feature/history` (worktree `C:\dev\ecad-features\history`), from `dev` at `b4a17783`
(021 container v2, 022 and CADCraft 0.19.0 merged).
Designer: **yes** (`docs/design/020.md`) · Size: L (the designer may split it into 020a/020b) ·
Tester: yes (the store and merge are risky areas).
Specs: EEP-0005 (as built), [`discussions/history-and-branches.md`](../discussions/history-and-branches.md)
(the contracts in §3 and §7 are agreed and normative here), [`discussions/revisions.md`](../discussions/revisions.md).

## Goal

The project's history is something the user sees and uses, the way Onshape shows it: a clear graph of
the main line with its versions and issued revisions, branches drawn from where they fork, and
simple actions on them. Electrical engineers neither know nor care about git, so the words are
ECAD's own: **version**, **revision**, **branch**, **merge**.

## What exists today

- **Container v2 (021):** the `branches` table with fork records; a `transactions` table keyed by
  (branch, seq); `versions` with kind user / revision / auto, and snapshots. Only `main` is
  created, and only the debugger's History, Storage and Branches views show any of it.
- **019:** Name Version and Issue Revision, with undo asking first, and revisions on the project
  manager. The PROJECT panel has Pages, Devices and Checks tabs, and the Properties panel.
- **The rules:** no lazy defaults (`ecad/CLAUDE.md`), so views have update maps. IEC is the
  default, never the law. Never a dead-end refusal.

## Scope

1. **History tab** in the PROJECT panel, beside Pages, Devices and Checks.
   - **The graph:**
     - the current branch's line, with its user versions and issued revisions (issued revisions
       stand out, as they are the fixed points);
     - other branches drawn from their fork point;
     - merges drawn as joins;
     - automatic snapshots never shown by their raw names: hide them, or show them faint as
       "Auto · <time>".
   - **Selecting a point** shows that state read-only: pages and devices as they were (reuse the
     debugger's state-at rebuild, off the live project), with its author, time and change summary.
   - **Actions:**
     - **Create version** (named snapshot, as today);
     - **Issue revision** (on the release line only, as 019);
     - **Restore this version**: appends a new transaction that returns the project to that state,
       never rewrites history, and can be undone;
     - **Branch from here.**
2. **Branches:** create (from the head or any version), rename, switch (the project shows that
   branch; the previous branch's unsaved state is handled safely), archive.
   - `main` is the release line and can't be archived or renamed.
   - Show the current branch everywhere it matters: the title, the banner and the History tab.
3. **Merge** a branch into `main` (and "update branch from `main`") following
   `history-and-branches.md` §3.5 and §7:
   - a three-way merge on state, landing after the latest revision;
   - crossing revisions is allowed and reported;
   - a preview first ("12 objects changed, 1 conflict"), then one transaction;
   - conflicts listed with a clear choice per object (keep `main` / keep branch / edit); never a
     dead end;
   - undoing a merge asks first.
   - The designer settles the open questions in §8 (the conflict UI, connection conflicts, what
     undoing a merge does to the source branch) and brings them to the overseer.
4. **Scaling (no lazy defaults):**
   - the graph comes from the `branches` and `versions` tables plus counts, never from reading
     the logs;
   - switching branch loads that branch's latest snapshot plus its tail (EEP-0005 open rules);
   - previewing a point uses the nearest snapshot plus a bounded replay;
   - merge cost scales with what changed since the fork, not with the project size;
   - each view gets an update map.
5. **Commands for agents:** MCP and typed equivalents for every action (`versions`, `branch`,
   `switch`, `merge` with preview), the same transactions as the clicks.
6. **Format:** branch and merge records go in the existing tables. Anything new in the format or
   ops is the **overseer's call**: stop and ask.

## Not in this brief

- More than one release line.
- Live multi-user editing, or sharing branches between copies ("combine two copies").
- Defrag and compaction UI.
- Git integration.
- The general look-and-feel pass. Use today's style. History must not be a bar at the bottom of
  the window (owner, 2026-11-09).

## Process

- Designer first (`docs/design/020.md`): flows, wireframes in words, the merge and conflict
  design, and the open questions with options and a recommendation. Then the owner approves.
- One worker on the feature, which keeps its context lean: logs to a file, and a state-note
  handover only near the context limit.
- A tester for merge and branch switching (store, risky).
- One PR, one reviewer, the owner's trial, and the owner merges.

## Acceptance

1. On the demo project: create a version, branch from it, change a device on the branch, switch
   back (main unchanged), merge with a preview, then issue Rev B. The graph shows it all correctly
   after save and reopen.
2. A conflict (the same device property changed on both sides) is shown and resolved in the
   merge dialog; undo of the merge asks first.
3. Restore an older version: one new transaction, undoable, and history is intact.
4. Large bench project: the History tab opens and updates within what the user would notice
   (about 100 ms). Switching branch costs about the same as opening (about 130 ms); a merge with
   few changes is under about 1 s.
5. Every action works from the command field and MCP with the same result as clicks.
