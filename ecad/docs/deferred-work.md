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
- **Stage 4 (checkpoints) is on hold** until stage 3 (libraries) and the core domain features
  are settled (owner, 2026-11-08). On the benchmark's large tier, about 13 s to open 170,000
  transactions is acceptable for now.

## Small performance follow-ups (found by briefs 012-014, owner 2026-11-08)

- **Every edit still goes through the whole document to find what is on a page.** After brief
  014, a move costs about 0.2 ms on small, 0.4 ms on medium and 1.2 ms on large (2026-11-08):
  pages are the same size, so the growth is whole-document scans, none of them in the
  translation. Redrawing the page after every edit (`ecad-cadcraft/src/project.rs`,
  `project_page_with`) goes through every item twice and every representation once to find
  those on the page: measured on large, projecting page 1 takes about 1 to 3 ms, of which those
  scans are 0.5 to 1.5 ms. `derive::terminal_positions` does the same, once per edit that moves a
  wire end (brief 014 lists it at most once per edit, `translate.rs` `ShownOnce`) and once per
  `wire` command. The index already exists inside `ecad-model` (brief 013's `page_items` and
  `page_reps`): expose read-only `items_on(page)` and `representations_on(page)` on `Document` and
  use them in projection and `terminal_positions`. Small, but it touches projection, so it needs
  its own brief.
- **Saves after the first grow with file size.** Measured on the large tier (2026-11-08): a
  save takes 68 ms at 43.5 MB and 224 ms at 163 MB. Two costs:
  - (a) **The whole verified prefix is re-read and hashed on every save**
    (`ecad-file/src/write.rs` save step 1, `prefix_hash`), so a change anywhere in the saved
    history refuses the save. Reading and hashing the file the same way takes 38 to 54 ms for
    43.5 MB and 140 ms for 163 MB: most of the save.
  - (b) **The identity reopen** (`ecad-file/src/disk.rs:370`): the file is reopened by path to
    check it has not been replaced, which a virus scanner may slow just after a write (not
    proven).

  Options, not implemented (changing a save-integrity check needs the owner): keep a metadata
  identity check (file ID, size, last-write time through `std::fs::metadata`); verify only the
  tail written since the last save, plus a sampled or chunked digest of the rest; or keep running
  chunk digests so only changed chunks are re-read. Any change must keep brief 002's replaced-file
  and deleted-and-recreated refusals.
