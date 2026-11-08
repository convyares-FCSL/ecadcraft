# Deferred work

Work the owner has decided to postpone, with what is needed to pick it up again. Nothing listed
here is implemented. Add an entry when work is deferred; remove it when the work is approved and
started.

## Upstream sync policy and cleanup (PR #5)

- **What:** how ECAD owns the root entry points, drops CADCraft clutter (its AutoCAD-parity
  planning documents, README screenshots, packaging, release and packaging-lint workflows) and
  stays mergeable with CADCraft. One policy file in three path categories (ECAD-owned, excluded
  upstream, upstream-maintained), an explicit `cargo xtask sync-upstream` run only on
  `upstream-sync/<date>` branches (no `merge=ours`, no merge drivers), and a CI check
  (`cargo xtask policy`).
- **Proposal:** [`deferred/upstream-sync-policy.md`](deferred/upstream-sync-policy.md), copied
  verbatim from commit `fc3eab6`.
- **References:**
  - PR #5, closed as deferred on 2026-10-15, not merged;
  - its head commit `8a6d10f`; the proposal itself is commit `fc3eab6`, the rest of #5's history
    is earlier PRs it was stacked on (already in `dev`);
  - branch `docs/upstream-sync-policy`, kept so the commits stay reachable.
- **Status:** direction approved with the earlier refinements (2026-10-12, `docs/decisions.md`);
  implementation deferred. Nothing has been deleted, moved or configured.
- **Before resuming:** re-check the proposed exclusion and keep lists against the current tree
  (the debugger, the Tool Sets seam X8 and later work have changed files outside `ecad/` since the
  proposal was written against `baee1b8`), then get the owner's approval for implementation.

## Also deferred (recorded elsewhere)

- Persistence stage 3 (libraries, `.ecadlib`) and its UI; stage 4 (checkpoints); migration
  rewriting of older files (E4); "Save a Copy As" with a new project ID (E9). See
  `docs/document-architecture-proposal.md` §22-§23.

## Small performance follow-ups (found by briefs 012-014, owner 2026-11-08)

- **`terminal_at` scans every representation.** Snapping a dragged wire end to a terminal
  (`ecad-cadcraft/src/translate.rs`, `terminal_at`) goes through every `Representation` in the
  document, because there is no page-indexed lookup. Add a `representations_on_page(page)` index
  in `ecad-model` (kept by `Op::apply`, checked in verify mode, like brief 013's indexes) and use
  it there. Small; do it the next time wire dragging is touched. Not on the move path.
- **Every save after the first costs about 30 ms on Windows.** `DiskProject::save` reopens the
  file by path before each save, to check that it has not been replaced
  (`ecad-file/src/disk.rs:370`). Just after a write, the virus scanner very likely rescans the
  changed file on that reopen; this isn't proven. It can't be noticed on Ctrl+S; it only showed in
  the benchmark's tight loop. If it is ever worth removing: check identity from metadata (file ID
  or last-write time through `std::fs::metadata`, which doesn't trigger a content scan), or keep the
  handle open with shared-read, exclusive-write access. Either change must keep brief 002's
  replaced-file and deleted-and-recreated refusals.
