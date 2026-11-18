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

## The commands in `history` and `merging` move to `cmd/` (lane 067 step 11)

- **What:** lane 067 step 11 moved ecad-cadcraft's commands into `cmd/` (`commands.rs` →
  `cmd/mod.rs`, each `*_cmds.rs` → `cmd/<area>.rs`, `plc_cmds/` → `cmd/plc/`, and `release.rs`'s
  `specs`, `run_prerelease` and `run_release` → `cmd/release.rs`). The `specs` and `run_*`
  functions of `history.rs` (`run_history`, `run_steps`, `run_pointinfo`, `run_version`,
  `run_showpoint`, `run_closepoint`, `run_restore`, `run_branches`, `run_branch`,
  `run_renamebranch`, `run_switchbranch`, `run_archivebranch`, `run_reopenbranch`), `merging.rs`
  (`run_merge`, `run_undomerge`) stayed: they use private helpers of their modules (`node_json`,
  `branch_json`, `kind_word`, `fork_of`, `from_text`, `unanswered_refusal`, …), so moving them
  would split logic or widen visibility, not be a clean move.
- **When:** with the R6 lane (typed command parameters, `docs/design/crates.md` §2.1), which
  rewrites each command's JSON form anyway; move them to `cmd/history.rs` and `cmd/merging.rs`
  then.
- **Status:** deferred by the lane 067 brief (2026-11-15); nothing else depends on it.

## Versioned built-in defaults: at the v1 format freeze (brief 066, owner 2026-11-15)

- **What:** while ECAD is pre-alpha there is one built-in defaults set, edited freely, and every
  project reads the current built-ins (a changed built-in moves every project that did not choose
  its own value). At the v1 format freeze that must stop. The design is kept in
  [`design/066-defaults-data.md`](design/066-defaults-data.md) §10:
  - numbered built-in sets that never change once released; a project names its set in
    `ProjectSettings.defaults` (format change F1), recorded in the "New project" transaction;
  - `sets.lock` and its test, which fails when a released value changes;
  - a `usedefaults` command and a Standards line "Built-in defaults: set *n* (newest: *m*)", to move
    a project to newer built-ins, with a **Keep how it looks** option;
  - a build that does not know a project's set refuses to open it.
- **Open calls then:** does a set freeze when it reaches `dev` or at a release (design 066 call 13)?
  Is **Keep how it looks** on by default (call 15)?

## Forward compatibility of stored words: at the v1 format freeze (brief 072, owner 2026-11-16)

- **What:** pre-v1, ECAD does not read files from newer builds or other tools. A word this build
  doesn't know (a style catalogue, a role, a device kind, a settings word) is a read error: the
  project opens read-only, the message names the word, the setting and where it is, and the file
  is never changed (design [`design/072-types.md`](design/072-types.md) §4.1). Enums are strict;
  there is no "keep the unknown word" variant.
- **Revisit at the v1 format freeze:** whether a v1 build keeps and writes back words from a newer
  build (and how a person sees them), and the **explicit import step** for files from other tools,
  which is where foreign words are mapped or refused.

## The part sets a device's rating: with a parts library (design 072 §2.2, owner 2026-11-18)

- **What:** when a part number's library entry carries a rating, it fills the device's typed
  rating fields; a value typed on the device is an override, and a Checks row reports an override
  that differs from the part.
- **Not built until** a parts library with ratings exists: no stub and no override handling now.
  Until then the typed rating fields are stored plainly (lane 072 B's rating step).

## Company standard file and later defaults-to-data moves (brief 066, owner 2026-11-15)

- **Company standard file** (design 066 §4): loading a house standard from a TOML file into the
  project's standard, as one undoable edit with the differences shown first. Waits for persistence
  stage 3 (libraries), where a standard also comes from a `.ecadlib`; it reuses 066's loader.
- **Later moves to data** (design 066 §2.3), each its own brief: house title block columns and
  rows (`title_block/house.rs`), cover page sizes (`front.rs`), the terminal strip drawing look
  (`strip_page.rs`), drawing line weights (`ecad-tokens/src/line.rs`, with plot styles).

## Also deferred (recorded elsewhere)

- Persistence stage 3 (libraries, `.ecadlib`) and its UI; stage 4 (checkpoints); migration
  rewriting of older files (E4); "Save a Copy As" with a new project ID (E9). See
  `docs/document-architecture-proposal.md` §22-§23.
- Delete `ProjectHandle::apply_ops` and the `test-ops` feature of `ecad-cadcraft` once brief
  010's `cableref` is merged: brief 009's tests build cable reference ends with it only because no
  command makes them on that branch (brief 009 review S3).
- **Stage 4 (checkpoints) is on hold** until stage 3 (libraries) and the core domain features
  are settled (owner, 2026-11-08). On the benchmark's large tier, about 13 s to open 170,000
  transactions is acceptable for now.
- Consider moving the definition content hash (`definition_hash`, canonical JSON) into
  `ecad-model`, so the electrical rules don't depend on the file format (types lane or later;
  067 step 4, owner option A).
- Remove ECAD's leftover wasm code now that ECAD is desktop-only (owner, 2026-11-15; design 065
  §1): the `cfg(target_arch = "wasm32")` branches (`ruzstd` decoding in `ecad-file`, the `chrono`
  clock, `agent_access`, `same-file`) and, as a separate CI task (house rule 11), the stale
  `targets: wasm32-unknown-unknown` line and the wasm step name in `.github/workflows/ci.yml`.
  The crate split (067 step 10) removed only `cargo xtask wasm` and its step in `lint` and `ci`.
- Pass the branch seed explicitly when the R9/types lanes touch `ProjectV2::create_branch`. The
  crate split (067 step 1c) left its signature alone: `BranchId::from_seed` takes the entropy
  from `ecad_store::random_u128` inside it (overseer, 2026-11-15).

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
